use std::{
    io::{Read, Write},
    net::{IpAddr, TcpListener},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use crate::{
    domain::{
        network_policy::{AllowedDestination, TransportLimits},
        provider::{
            ModelId, ModelProvider, ProviderFailure, ProviderHealth, ProviderId,
            RuntimeExecutionInput,
        },
    },
    infrastructure::providers::anthropic::{
        evaluate_destination, AnthropicAdapter, AnthropicConfig,
    },
};

const TEST_CREDENTIAL: &str = "sk-ant-test-7712bd-CANARY-NEVER-LEAK";

#[derive(Debug, Clone)]
struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl CapturedRequest {
    fn path(&self) -> &str {
        self.request_line.split_whitespace().nth(1).unwrap_or("")
    }

    fn method(&self) -> &str {
        self.request_line.split_whitespace().next().unwrap_or("")
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn contains_credential(&self) -> bool {
        self.request_line.contains(TEST_CREDENTIAL)
            || self.body.contains(TEST_CREDENTIAL)
            || self.headers.iter().any(|(name, value)| {
                name.contains(TEST_CREDENTIAL) || value.contains(TEST_CREDENTIAL)
            })
    }
}

fn start_fixture(
    status: &str,
    content_type: &str,
    body: &str,
    delay: Duration,
) -> (u16, Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("خادم الاختبار المحلي يجب أن يرتبط");
    let port = listener
        .local_addr()
        .expect("عنوان خادم الاختبار يجب أن يتاح")
        .port();
    let status = status.to_owned();
    let content_type = content_type.to_owned();
    let body = body.to_owned();
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut received = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    received.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&received);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let content_length = text[..header_end]
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.trim()
                                    .eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if received.len() >= header_end + 4 + content_length {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }

        let text = String::from_utf8_lossy(&received).to_string();
        let (head, request_body) = match text.split_once("\r\n\r\n") {
            Some((head, body)) => (head.to_owned(), body.to_owned()),
            None => (text.clone(), String::new()),
        };
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default().to_owned();
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect::<Vec<_>>();
        let _ = sender.send(CapturedRequest {
            request_line,
            headers,
            body: request_body,
        });

        thread::sleep(delay);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    (port, receiver)
}

fn loopback_destination(port: u16) -> AllowedDestination {
    AllowedDestination::for_tests(
        "http",
        "127.0.0.1",
        port,
        vec![IpAddr::from([127, 0, 0, 1])],
        true,
        TransportLimits::default(),
    )
}

fn provider_id() -> ProviderId {
    ProviderId::new("ANTHROPIC_PRIMARY").expect("معرّف الاختبار يجب أن يكون صالحاً")
}

fn adapter(port: u16, credential: Option<&str>) -> AnthropicAdapter {
    AnthropicAdapter::for_test(
        provider_id(),
        AnthropicConfig::new(loopback_destination(port), credential.map(str::to_owned)),
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
}

fn runtime_input(text: &str) -> RuntimeExecutionInput {
    RuntimeExecutionInput::new(text).expect("إدخال الاختبار يجب أن يكون صالحاً")
}

/// جسم استجابة Message بشكل مطابق لتعريفات حزمة Anthropic الرسمية.
const MESSAGE_RESPONSE: &str = r#"{
  "id": "msg_013Zva2CMHLNnXjNJJKqJ2EF",
  "type": "message",
  "role": "assistant",
  "model": "claude-sonnet-4-5",
  "content": [{"type": "text", "text": "Bounded answer.", "citations": null}],
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {"input_tokens": 12, "output_tokens": 7}
}"#;

/// جسم صفحة ModelInfo بشكل مطابق للتعريف الرسمي (data/has_more/first_id/last_id).
const MODELS_PAGE: &str = r#"{
  "data": [
    {
      "id": "claude-sonnet-4-5",
      "type": "model",
      "display_name": "Claude Sonnet 4.5",
      "created_at": "2025-05-14T00:00:00Z",
      "max_input_tokens": 200000,
      "max_tokens": 64000,
      "lifecycle": "active",
      "capabilities": {"image_input": {"supported": true}, "structured_outputs": {"supported": true}}
    }
  ],
  "has_more": false,
  "first_id": "claude-sonnet-4-5",
  "last_id": "claude-sonnet-4-5"
}"#;

// ===== سياسة الوجهة =====

#[test]
fn applies_the_destination_policy_to_the_anthropic_endpoint() {
    assert_eq!(
        evaluate_destination("http://api.anthropic.com"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN"
        ))
    );
    assert_eq!(
        evaluate_destination("https://key:secret@api.anthropic.com"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_CREDENTIALS_IN_URL"
        ))
    );
    assert_eq!(
        evaluate_destination("https://169.254.169.254"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_DESTINATION_DENIED"
        ))
    );
}

// ===== الاكتشاف =====

#[test]
fn discovery_reads_the_anthropic_models_page_and_sends_both_required_headers() {
    let (port, requests) = start_fixture("200 OK", "application/json", MODELS_PAGE, Duration::ZERO);
    let models = adapter(port, Some(TEST_CREDENTIAL))
        .discover_models()
        .expect("الاكتشاف يجب أن ينجح");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id.as_str(), "claude-sonnet-4-5");
    // الاسم المعروض من الموفر لا المعرّف الخام.
    assert_eq!(models[0].display_name, "Claude Sonnet 4.5");
    assert_eq!(
        models[0].modified_at.as_deref(),
        Some("2025-05-14T00:00:00Z")
    );
    // لم تُتحقق قدرة أدوات العميل — لا ادعاء.
    assert!(!models[0].supports_tools);

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(captured.method(), "GET");
    assert_eq!(captured.path(), "/v1/models");
    // الترويسة المميزة لـ Anthropic: x-api-key لا Bearer.
    assert_eq!(captured.header("x-api-key"), Some(TEST_CREDENTIAL));
    assert!(captured.header("authorization").is_none());
    // نسخة الواجهة إلزامية في كل طلب.
    assert_eq!(captured.header("anthropic-version"), Some("2023-06-01"));
}

#[test]
fn discovery_sends_no_credential_header_when_none_is_stored() {
    let (port, requests) = start_fixture("200 OK", "application/json", MODELS_PAGE, Duration::ZERO);
    adapter(port, None)
        .discover_models()
        .expect("الاكتشاف يجب أن ينجح");

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert!(captured.header("x-api-key").is_none());
    assert!(captured.header("authorization").is_none());
    assert!(!captured.contains_credential());
}

#[test]
fn discovery_rejects_a_model_entry_with_an_invalid_identifier() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[{\"id\":\"../../outside\",\"display_name\":\"x\"}],\"has_more\":false}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== التوليد =====

#[test]
fn generation_posts_the_anthropic_message_shape_with_max_tokens() {
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        MESSAGE_RESPONSE,
        Duration::ZERO,
    );
    let generation = adapter(port, Some(TEST_CREDENTIAL))
        .generate_non_streaming(
            &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
            &runtime_input("hello"),
        )
        .expect("التوليد يجب أن ينجح");

    assert_eq!(generation.output, "Bounded answer.");
    assert_eq!(generation.model_id.as_str(), "claude-sonnet-4-5");

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(captured.method(), "POST");
    assert_eq!(captured.path(), "/v1/messages");
    assert_eq!(captured.header("x-api-key"), Some(TEST_CREDENTIAL));
    assert_eq!(captured.header("anthropic-version"), Some("2023-06-01"));
    // `max_tokens` إلزامي في واجهة الرسائل — يجب أن يكون في كل طلب.
    assert!(captured.body.contains("\"max_tokens\":4096"));
    assert!(captured.body.contains("\"model\":\"claude-sonnet-4-5\""));
    assert!(captured.body.contains("\"role\":\"user\""));
    assert!(captured.body.contains("\"stream\":false"));
    // لا `system` ولا حقول OpenAI غير المعروفة في واجهة Anthropic.
    assert!(!captured.body.contains("chat/completions"));
}

#[test]
fn generation_joins_multiple_text_blocks_and_stays_bounded() {
    let body = r#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-sonnet-4-5","content":[{"type":"text","text":"first "},{"type":"text","text":"second"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#;
    let (port, _requests) = start_fixture("200 OK", "application/json", body, Duration::ZERO);
    let generation = adapter(port, None)
        .generate_non_streaming(
            &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
            &runtime_input("hello"),
        )
        .expect("التوليد يجب أن ينجح");
    assert_eq!(generation.output, "first second");
}

#[test]
fn generation_rejects_non_text_content_blocks() {
    // كتلة tool_use: الإجابة ليست نصاً بسيطاً ⇒ فشل مغلق بلا تفسير جزئي.
    for block in [
        r#"{"type":"tool_use","id":"toolu_1","name":"do","input":{}}"#,
        r#"{"type":"thinking","thinking":"hmm","signature":"sig"}"#,
        r#"{"type":"redacted_thinking","data":"opaque"}"#,
        r#"{"type":"text"}"#,
    ] {
        let body = format!(
            "{{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-5\",\"content\":[{block}],\"stop_reason\":\"end_turn\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":2}}}}"
        );
        let (port, _requests) = start_fixture("200 OK", "application/json", &body, Duration::ZERO);
        assert_eq!(
            adapter(port, None).generate_non_streaming(
                &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
                &runtime_input("hello")
            ),
            Err(ProviderFailure::InvalidResponse),
            "الكتلة {block} يجب أن تُرفض"
        );
    }
}

#[test]
fn generation_rejects_a_different_role_or_model() {
    for body in [
        r#"{"id":"m","type":"message","role":"user","model":"claude-sonnet-4-5","content":[{"type":"text","text":"x"}],"stop_reason":"end_turn","usage":{}}"#,
        r#"{"id":"m","type":"message","role":"assistant","model":"claude-opus-4-1","content":[{"type":"text","text":"x"}],"stop_reason":"end_turn","usage":{}}"#,
        r#"{"id":"m","type":"message","role":"assistant","model":"claude-sonnet-4-5","content":[],"stop_reason":"end_turn","usage":{}}"#,
    ] {
        let (port, _requests) = start_fixture("200 OK", "application/json", body, Duration::ZERO);
        assert_eq!(
            adapter(port, None).generate_non_streaming(
                &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
                &runtime_input("hello")
            ),
            Err(ProviderFailure::InvalidResponse)
        );
    }
}

// العيبة المُثبتة بالوثائق: واجهة Anthropic تُعيد لقطة النموذج المؤرّخة عند طلب
// اسم مستعار. المساواة الحرفية وحدها كانت ترفض استجابة سليمة.
#[test]
fn generation_accepts_the_documented_dated_snapshot_echo() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        r#"{"id":"m","type":"message","role":"assistant","model":"claude-sonnet-4-5-20250929","content":[{"type":"text","text":"Bounded answer."}],"stop_reason":"end_turn","usage":{}}"#,
        Duration::ZERO,
    );
    let generation = adapter(port, None)
        .generate_non_streaming(
            &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
            &runtime_input("hello"),
        )
        .expect("اللقطة المؤرّخة الموثّقة يجب أن تُقبل");
    assert_eq!(generation.output, "Bounded answer.");
    // الهوية المُبلَّغة تبقى اللقطة الفعلية لا الاسم المستعار.
    assert_eq!(generation.model_id.as_str(), "claude-sonnet-4-5-20250929");

    // عائلة أخرى تبقى مرفوضة: لا مطابقة بالبادئة وحدها.
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        r#"{"id":"m","type":"message","role":"assistant","model":"claude-sonnet-4-5-latest","content":[{"type":"text","text":"x"}],"stop_reason":"end_turn","usage":{}}"#,
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
            &runtime_input("hello")
        ),
        Err(ProviderFailure::InvalidResponse)
    );
}

#[test]
fn generation_rejects_a_body_above_the_bounded_limit() {
    let padding = "a".repeat(64 * 1024 + 1024);
    let body = format!(
        "{{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-5\",\"content\":[{{\"type\":\"text\",\"text\":\"{padding}\"}}],\"stop_reason\":\"end_turn\",\"usage\":{{}}}}"
    );
    let (port, _requests) = start_fixture("200 OK", "application/json", &body, Duration::ZERO);
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("claude-sonnet-4-5").expect("معرّف"),
            &runtime_input("hello")
        ),
        Err(ProviderFailure::OutputTooLarge)
    );
}

// ===== المسارات المغلقة والنقل =====

#[test]
fn tool_path_stays_fail_closed_for_anthropic() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        MESSAGE_RESPONSE,
        Duration::ZERO,
    );
    let provider = adapter(port, None);
    let model_id = ModelId::new("claude-sonnet-4-5").expect("معرّف");
    assert_eq!(
        provider.generate_with_tools(&model_id, &runtime_input("hello"), &[]),
        Err(ProviderFailure::ToolCallUnsupported)
    );
    assert_eq!(
        provider.generate_structured(&model_id, &runtime_input("hello")),
        Err(ProviderFailure::InvalidResponse)
    );
}

#[test]
fn reports_health_only_after_a_valid_models_page() {
    let (port, _requests) =
        start_fixture("200 OK", "application/json", MODELS_PAGE, Duration::ZERO);
    assert_eq!(adapter(port, None).health(), ProviderHealth::Available);

    let (port, _requests) =
        start_fixture("500 Server Error", "application/json", "{}", Duration::ZERO);
    assert_eq!(adapter(port, None).health(), ProviderHealth::Unavailable);
}

#[test]
fn rejects_a_non_json_content_type_and_a_malformed_body() {
    let (port, _requests) = start_fixture("200 OK", "text/plain", MODELS_PAGE, Duration::ZERO);
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );

    let (port, _requests) =
        start_fixture("200 OK", "application/json", "{\"data\":", Duration::ZERO);
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );
}

#[test]
fn maps_a_slow_response_to_timeout_without_exposing_transport_detail() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        MODELS_PAGE,
        Duration::from_millis(600),
    );
    let provider = AnthropicAdapter::for_test(
        provider_id(),
        AnthropicConfig::new(loopback_destination(port), Some(TEST_CREDENTIAL.to_owned())),
        Duration::from_millis(80),
        Duration::from_millis(80),
    );
    let failure = provider.discover_models().expect_err("يجب أن يفشل");
    assert_eq!(failure, ProviderFailure::Timeout);
    assert!(!failure.to_string().contains(TEST_CREDENTIAL));
    assert!(!failure.to_string().contains("127.0.0.1"));
}

#[test]
fn maps_unauthorized_to_credential_missing_without_echoing_the_key() {
    let (port, _requests) = start_fixture(
        "401 Unauthorized",
        "application/json",
        "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key sk-ant-test-7712bd-CANARY-NEVER-LEAK\"}}",
        Duration::ZERO,
    );
    let failure = adapter(port, Some(TEST_CREDENTIAL))
        .discover_models()
        .expect_err("يجب أن يفشل");
    assert_eq!(failure, ProviderFailure::CredentialMissing);
    assert!(!failure.to_string().contains(TEST_CREDENTIAL));
    assert!(!failure.to_string().contains("authentication_error"));
}

#[test]
fn every_reported_error_string_is_free_of_the_credential_and_the_endpoint() {
    for failure in [
        ProviderFailure::Unavailable,
        ProviderFailure::Timeout,
        ProviderFailure::InvalidResponse,
        ProviderFailure::OutputTooLarge,
        ProviderFailure::ToolCallUnsupported,
        ProviderFailure::CredentialMissing,
        ProviderFailure::EndpointRejected("NETWORK_DESTINATION_DENIED"),
    ] {
        let text = failure.to_string();
        for forbidden in [
            TEST_CREDENTIAL,
            "x-api-key",
            "Bearer",
            "sk-ant-",
            "127.0.0.1",
            "http",
        ] {
            assert!(
                !text.contains(forbidden),
                "تسرب «{forbidden}» في نص الخطأ: {text}"
            );
        }
    }
}
