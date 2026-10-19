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
    infrastructure::providers::openai_compatible::{
        evaluate_destination, OpenAiCompatibleAdapter, OpenAiCompatibleConfig,
    },
};

// مفتاح اختبار صريح — وجوده في أي خطأ أو سجل يفشل الاختبار.
const TEST_CREDENTIAL: &str = "sk-test-4f2a9c-CANARY-NEVER-LEAK";

/// طلب ملتقط من الخادم الوهمي — نتحقق من المسار والترويسة والجسم فعلياً.
#[derive(Debug, Clone)]
struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl CapturedRequest {
    // مسار الطلب يُستَخدم في تحقّقات المسارات المقصودة؛ يُبقى مُعلَّماً صراحةً حتى لا
    // يظهر تحذير شيفرة ميتة في بناء الاختبارات عندما تُخفَّف بعض التحقّقات.
    #[allow(dead_code)]
    fn path(&self) -> &str {
        self.request_line.split_whitespace().nth(1).unwrap_or("")
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

/// خادم HTTP محلي وهمي: يقبل اتصالاً واحداً، يلتقط الطلب كاملاً، ثم يرد.
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
        // استهلاك الطلب كاملاً قبل الرد (نفس نمط اختبارات Ollama) — يجنب RST.
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

/// وجهة حلقية مبنية يدوياً: الاختبار يستهدف منفذاً عشوائياً للخادم الوهمي.
/// سياسة الوجهة لها اختباراتها المستقلة؛ هنا نبني الوجهة مباشرة لنعزل المحوّل.
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
    ProviderId::new("OPENAI_PRIMARY").expect("معرّف الاختبار يجب أن يكون صالحاً")
}

fn adapter(port: u16, credential: Option<&str>) -> OpenAiCompatibleAdapter {
    OpenAiCompatibleAdapter::for_test(
        provider_id(),
        OpenAiCompatibleConfig::new(
            loopback_destination(port),
            "/v1/models",
            "/v1/chat/completions",
            credential.map(str::to_owned),
        ),
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
}

/// محوّل OpenRouter: **نفس** مساري OpenAI المتوافقين الموثّقين
/// (`/api/v1/models` و`/api/v1/chat/completions`)؛ الفرق الوحيد في الإنتاج هو
/// الأساس `https://openrouter.ai/api`. هذا الغلاف يجعل نيّة الاختبار صريحة.
fn adapter_openrouter(port: u16) -> OpenAiCompatibleAdapter {
    adapter(port, Some(TEST_CREDENTIAL))
}

fn runtime_input(text: &str) -> RuntimeExecutionInput {
    RuntimeExecutionInput::new(text).expect("إدخال الاختبار يجب أن يكون صالحاً")
}

// ===== سياسة الوجهة مطبَّقة على المحوّل السحابي =====

#[test]
fn rejects_non_loopback_http_and_credential_bearing_base_urls() {
    // http إلى مضيف عام: مرفوض.
    assert_eq!(
        evaluate_destination("http://api.example.com"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN"
        ))
    );
    // بيانات اعتماد في العنوان: مرفوضة (وهي أشهر تسريب للمفاتيح).
    assert_eq!(
        evaluate_destination("https://user:secret@api.example.com"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_CREDENTIALS_IN_URL"
        ))
    );
    // مخطط غير مدعوم: مرفوض.
    assert_eq!(
        evaluate_destination("ftp://api.example.com"),
        Err(ProviderFailure::EndpointRejected(
            "NETWORK_SCHEME_NOT_ALLOWED"
        ))
    );
    // https عام: مقبول (قد يفشل الحلّ على جهاز بلا شبكة فيُقبل خطأ الحلّ وحده).
    match evaluate_destination("https://api.openai.com") {
        Ok(destination) => {
            assert_eq!(destination.host(), "api.openai.com");
            assert_eq!(destination.port(), 443);
            assert!(destination.is_secure());
        }
        Err(ProviderFailure::EndpointRejected("NETWORK_HOST_UNRESOLVED")) => {}
        Err(other) => panic!("رفض غير متوقع لعنوان https عام: {other}"),
    }
}

// ===== الاكتشاف =====

#[test]
fn discovery_reads_the_openai_models_shape_and_sends_no_credential_when_absent() {
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"object\":\"list\",\"data\":[{\"id\":\"gpt-4o-mini\",\"object\":\"model\",\"created\":1735689600},{\"id\":\"gpt-4o\",\"object\":\"model\",\"created\":1735689600}]}",
        Duration::ZERO,
    );
    let provider = adapter(port, None);
    let models = provider.discover_models().expect("الاكتشاف يجب أن ينجح");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id.as_str(), "gpt-4o-mini");
    assert_eq!(models[1].id.as_str(), "gpt-4o");
    // العقد الشائع لا يعلن قدرة أدوات: لا ادعاء.
    assert!(!models[0].supports_tools);
    assert_eq!(
        models[0].modified_at.as_deref(),
        Some("2025-01-01T00:00:00+00:00")
    );

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(captured.request_line, "GET /v1/models HTTP/1.1");
    // بلا مفتاح محفوظ ⇒ لا ترويسة مصادقة إطلاقاً.
    assert!(captured.header("authorization").is_none());
    assert!(!captured.contains_credential());
}

#[test]
fn discovery_sends_the_bearer_credential_when_configured() {
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[{\"id\":\"gpt-4o-mini\"}]}",
        Duration::ZERO,
    );
    let provider = adapter(port, Some(TEST_CREDENTIAL));
    provider.discover_models().expect("الاكتشاف يجب أن ينجح");

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(
        captured.header("authorization"),
        Some(format!("Bearer {TEST_CREDENTIAL}").as_str())
    );
}

#[test]
fn discovery_reports_health_only_after_a_valid_models_response() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[]}",
        Duration::ZERO,
    );
    assert_eq!(adapter(port, None).health(), ProviderHealth::Available);
}

#[test]
fn discovery_rejects_a_model_entry_with_an_invalid_identifier() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[{\"id\":\"../../etc/passwd\"}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== التوليد =====

#[test]
fn generation_posts_the_chat_shape_and_returns_the_single_choice_content() {
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-mini\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Bounded answer.\"},\"finish_reason\":\"stop\"}]}",
        Duration::ZERO,
    );
    let provider = adapter(port, Some(TEST_CREDENTIAL));
    let generation = provider
        .generate_non_streaming(
            &ModelId::new("gpt-4o-mini").expect("معرّف"),
            &runtime_input("hi"),
        )
        .expect("التوليد يجب أن ينجح");

    assert_eq!(generation.output, "Bounded answer.");
    assert_eq!(generation.model_id.as_str(), "gpt-4o-mini");

    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(captured.request_line, "POST /v1/chat/completions HTTP/1.1");
    assert_eq!(
        captured.header("authorization"),
        Some(format!("Bearer {TEST_CREDENTIAL}").as_str())
    );
    assert!(captured
        .header("content-type")
        .is_some_and(|value| value.starts_with("application/json")));
    assert!(captured.body.contains("\"model\":\"gpt-4o-mini\""));
    assert!(captured.body.contains("\"stream\":false"));
    assert!(captured.body.contains("\"role\":\"user\""));
}

#[test]
fn generation_rejects_an_echoed_model_that_differs_from_the_requested_one() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"some-other-model\",\"choices\":[{\"message\":{\"content\":\"x\"}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("gpt-4o-mini").expect("معرّف"),
            &runtime_input("hi")
        ),
        Err(ProviderFailure::InvalidResponse)
    );
}

// العيبة المُثبتة بالوثائق (19 أكتوبر 2026): OpenAI وxAI تُعيدان في `model`
// **لقطة النموذج المُنفَّذة** لا الاسم المستعار المطلوب. المساواة الحرفية وحدها
// كانت ترفض استجابة سليمة تماماً ⇒ فشل توليد كاذب لدى مزوّد سليم.
#[test]
fn generation_accepts_the_documented_snapshot_alias_in_the_model_echo() {
    // OpenAI: طلب `gpt-4o` ⇒ استجابة موثّقة `gpt-4o-2024-08-06`.
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-2024-08-06\",\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Bounded answer.\"}}]}",
        Duration::ZERO,
    );
    let generation = adapter(port, Some(TEST_CREDENTIAL))
        .generate_non_streaming(&ModelId::new("gpt-4o").expect("معرّف"), &runtime_input("hi"))
        .expect("اللقطة الموثّقة يجب أن تُقبل");
    assert_eq!(generation.output, "Bounded answer.");
    // الهوية المُبلَّغة تبقى **اللقطة الفعلية** التي أعلنها الموفر — لا الاسم المستعار.
    assert_eq!(generation.model_id.as_str(), "gpt-4o-2024-08-06");
    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert!(captured.body.contains("\"model\":\"gpt-4o\""));

    // xAI: `grok-2` ⇒ `grok-2-1212`.
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"grok-2-1212\",\"choices\":[{\"message\":{\"content\":\"ok\"}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None)
            .generate_non_streaming(&ModelId::new("grok-2").expect("معرّف"), &runtime_input("hi"))
            .expect("لقطة xAI يجب أن تُقبل")
            .model_id
            .as_str(),
        "grok-2-1212"
    );

    // نموذج مختلف فعلاً يبقى مرفوضاً: `gpt-4o` ⇒ `gpt-4o-mini` ليس لقطة.
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-mini\",\"choices\":[{\"message\":{\"content\":\"x\"}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None)
            .generate_non_streaming(&ModelId::new("gpt-4o").expect("معرّف"), &runtime_input("hi")),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== عقد OpenRouter الموثّق (بدون مفتاح ولا نداء خارجي) =====
// الشكل مأخوذ من وثائق OpenRouter الرسمية: GET /api/v1/models يُرجع
// {data:[{id:"vendor/model"}], total_count} وPOST /api/v1/chat/completions
// يُرجع choices[0].message.content مع model بالمعرّف نفسه.
#[test]
fn openrouter_documented_model_catalog_and_chat_contract_parse() {
    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[{\"id\":\"openai/gpt-4.1\",\"name\":\"OpenAI: GPT-4.1\"},{\"id\":\"~openai/gpt-astra-latest\"},{\"id\":\"nvidia/nemotron-3.5-super-vl:free\"}],\"total_count\":3}",
        Duration::ZERO,
    );
    let models = adapter_openrouter(port)
        .discover_models()
        .expect("فهرس OpenRouter الموثّق يجب أن يُحلَّل");
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].id.as_str(), "openai/gpt-4.1");
    assert_eq!(models[1].id.as_str(), "~openai/gpt-astra-latest");
    assert_eq!(models[2].id.as_str(), "nvidia/nemotron-3.5-super-vl:free");
    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    // الأساس https://openrouter.ai/api + المسار الموثّق /v1/models.
    assert_eq!(captured.request_line, "GET /v1/models HTTP/1.1");
    assert_eq!(
        captured.header("authorization"),
        Some(format!("Bearer {TEST_CREDENTIAL}").as_str())
    );

    let (port, requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"id\":\"gen-1\",\"model\":\"openai/gpt-4.1\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Grounded answer.\"},\"finish_reason\":\"stop\"}]}",
        Duration::ZERO,
    );
    let generation = adapter_openrouter(port)
        .generate_non_streaming(
            &ModelId::new("openai/gpt-4.1").expect("معرّف"),
            &runtime_input("hi"),
        )
        .expect("استجابة OpenRouter الموثّقة يجب أن تُقبل");
    assert_eq!(generation.output, "Grounded answer.");
    let captured = requests.recv().expect("الطلب يجب أن يُلتقط");
    assert_eq!(captured.request_line, "POST /v1/chat/completions HTTP/1.1");
    assert!(captured.body.contains("\"stream\":false"));
}

// أخطاء موثّقة: OpenRouter يُرجع {error:{message,code}}، وOpenAI 400/404/429/500.
// القاعدة: تصنيف آمن بلا كشف جسم الخطأ ولا المفتاح، وفشل مغلق دائماً.
#[test]
fn documented_error_statuses_map_to_safe_failures_without_leaking_the_body() {
    let cases = [
        ("400 Bad Request", ProviderFailure::Unavailable),
        ("404 Not Found", ProviderFailure::Unavailable),
        ("429 Too Many Requests", ProviderFailure::Unavailable),
        ("500 Internal Server Error", ProviderFailure::Unavailable),
        ("503 Service Unavailable", ProviderFailure::Unavailable),
    ];
    for (status, expected) in cases {
        let body = "{\"error\":{\"message\":\"rate limited for sk-test-4f2a9c-CANARY-NEVER-LEAK\",\"code\":429}}";
        let (port, _requests) = start_fixture(status, "application/json", body, Duration::ZERO);
        let failure = adapter(port, Some(TEST_CREDENTIAL))
            .generate_non_streaming(
                &ModelId::new("gpt-4o-mini").expect("معرّف"),
                &runtime_input("hi"),
            )
            .expect_err("حالة الخطأ يجب أن تفشل مغلقة");
        assert_eq!(failure, expected, "الحالة {status}");
        let text = failure.to_string();
        assert!(!text.contains(TEST_CREDENTIAL), "تسرب مفتاح في {status}");
        assert!(!text.contains("rate limited"), "تسرب جسم في {status}");
    }
}

#[test]
fn generation_rejects_tool_calls_on_the_plain_text_path() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-mini\",\"choices\":[{\"message\":{\"content\":null,\"tool_calls\":[{\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"do\",\"arguments\":\"{}\"}}]}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("gpt-4o-mini").expect("معرّف"),
            &runtime_input("hi")
        ),
        Err(ProviderFailure::ToolCallUnsupported)
    );
}

#[test]
fn generation_rejects_multiple_choices_and_empty_content() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-mini\",\"choices\":[{\"message\":{\"content\":\"a\"}},{\"message\":{\"content\":\"b\"}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("gpt-4o-mini").expect("معرّف"),
            &runtime_input("hi")
        ),
        Err(ProviderFailure::InvalidResponse)
    );

    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"model\":\"gpt-4o-mini\",\"choices\":[{\"message\":{\"content\":\"\"}}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).generate_non_streaming(
            &ModelId::new("gpt-4o-mini").expect("معرّف"),
            &runtime_input("hi")
        ),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== السياسة على المسارات المغلقة =====

#[test]
fn tool_and_structured_paths_stay_fail_closed_by_default() {
    let tools = Vec::new();
    // حتى بلا خادم: الرفض يأتي من العقد الافتراضي لا من الشبكة.
    let (port, _requests) = start_fixture("200 OK", "application/json", "{}", Duration::ZERO);
    let provider = adapter(port, None);
    let model_id = ModelId::new("gpt-4o-mini").expect("معرّف");
    assert_eq!(
        provider.generate_with_tools(&model_id, &runtime_input("hi"), &tools),
        Err(ProviderFailure::ToolCallUnsupported)
    );
    assert_eq!(
        provider.generate_structured(&model_id, &runtime_input("hi")),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== حدود الحجم والنوع =====

#[test]
fn rejects_a_response_body_above_the_bounded_limit() {
    // جسم صالح JSON لكنه يتجاوز 1MB — يجب ألا يُقرأ كاملاً.
    let padding = "a".repeat(1024 * 1024 + 1024);
    let body = format!("{{\"data\":[{{\"id\":\"{padding}\"}}]}}");
    let (port, _requests) = start_fixture("200 OK", "application/json", &body, Duration::ZERO);

    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::OutputTooLarge)
    );
}

#[test]
fn rejects_a_non_json_content_type_even_with_a_valid_body() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "text/html",
        "{\"data\":[{\"id\":\"gpt-4o-mini\"}]}",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );
}

#[test]
fn rejects_a_json_content_type_with_a_malformed_body() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[{\"id\":",
        Duration::ZERO,
    );
    assert_eq!(
        adapter(port, None).discover_models(),
        Err(ProviderFailure::InvalidResponse)
    );
}

// ===== المهلة والفشل المغلق =====

#[test]
fn maps_a_slow_response_to_timeout_without_exposing_transport_detail() {
    let (port, _requests) = start_fixture(
        "200 OK",
        "application/json",
        "{\"data\":[]}",
        Duration::from_millis(600),
    );
    let provider = OpenAiCompatibleAdapter::for_test(
        provider_id(),
        OpenAiCompatibleConfig::new(
            loopback_destination(port),
            "/v1/models",
            "/v1/chat/completions",
            Some(TEST_CREDENTIAL.to_owned()),
        ),
        Duration::from_millis(80),
        Duration::from_millis(80),
    );

    let failure = provider
        .discover_models()
        .expect_err("الاستجابة البطيئة يجب أن تفشل");
    assert_eq!(failure, ProviderFailure::Timeout);
    assert!(!failure.to_string().contains(TEST_CREDENTIAL));
    assert!(!failure.to_string().contains("127.0.0.1"));
    assert!(!failure.to_string().contains(&port.to_string()));
}

#[test]
fn reports_unavailable_when_no_service_accepts_the_connection() {
    // منفذ مُغلَق محلياً: نربط ثم نُسقط المستمع — المنفذ صار حراً والاتصال يُرفض.
    // (الاعتماد على منفذ ثابت محجوز مثل 1 يجعل الاختبار رهينة سياسة المنافذ
    // في نظام التشغيل: الاتصال هناك يُرفض بصلاحية لا بعدم وجود خدمة.)
    let closed_port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("ربط مؤقت");
        listener.local_addr().expect("العنوان").port()
    };
    let provider = adapter(closed_port, Some(TEST_CREDENTIAL));
    let failure = provider
        .discover_models()
        .expect_err("غياب الخدمة يجب أن يفشل");
    // كلٌّ من الرفض الفوري (لا خدمة تستمع) والمهلة فشل مغلق مقبول: نظام
    // التشغيل قد يؤجّل إشارة الرفض. المهم ألا يكون نجاحاً وألا يسرّب شيئاً.
    assert!(
        matches!(
            failure,
            ProviderFailure::Unavailable | ProviderFailure::Timeout
        ),
        "الفشل المتوقع غير متاح/مهلة، وُجد: {failure}"
    );
    assert!(!failure.to_string().contains(TEST_CREDENTIAL));
    assert!(!failure.to_string().contains(&closed_port.to_string()));
}

#[test]
fn maps_unauthorized_and_forbidden_to_credential_missing_without_echoing_the_key() {
    for status in ["401 Unauthorized", "403 Forbidden"] {
        let (port, _requests) = start_fixture(
            status,
            "application/json",
            "{\"error\":{\"message\":\"invalid api key sk-test-4f2a9c-CANARY-NEVER-LEAK\"}}",
            Duration::ZERO,
        );
        let failure = adapter(port, Some(TEST_CREDENTIAL))
            .discover_models()
            .expect_err("الرفض يجب أن يفشل");
        assert_eq!(failure, ProviderFailure::CredentialMissing);
        // لا نص الخادم ولا المفتاح يظهران في أي خطأ.
        assert!(!failure.to_string().contains(TEST_CREDENTIAL));
        assert!(!failure.to_string().contains("invalid api key"));
    }
}

#[test]
fn every_reported_error_string_is_free_of_the_credential_and_the_endpoint() {
    // مسح شامل: كل رمز فشل ممكن من هذا المحوّل يجب أن يكون نصاً ثابتاً.
    let failures = [
        ProviderFailure::Unavailable,
        ProviderFailure::Timeout,
        ProviderFailure::InvalidResponse,
        ProviderFailure::ModelUnavailable,
        ProviderFailure::InputTooLarge,
        ProviderFailure::OutputTooLarge,
        ProviderFailure::ToolCallUnsupported,
        ProviderFailure::EndpointRejected("NETWORK_DESTINATION_DENIED"),
        ProviderFailure::CredentialMissing,
    ];
    for failure in failures {
        let text = failure.to_string();
        assert!(!text.contains(TEST_CREDENTIAL), "تسرب المفتاح في {text}");
        assert!(!text.contains("Bearer"), "تسرب نمط الترويسة في {text}");
        assert!(!text.contains("127.0.0.1"), "تسرب عنوان في {text}");
        assert!(!text.contains("sk-"), "تسرب نمط مفتاح في {text}");
        assert!(!text.contains("http"), "تسرب مخطط/عنوان في {text}");
    }
}
