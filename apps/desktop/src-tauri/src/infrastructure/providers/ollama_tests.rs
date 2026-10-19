use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

use crate::{
    domain::provider::{
        ModelId, ModelProvider, ProviderFailure, ProviderHealth, ProviderToolDefinition,
        ProviderToolResponse, RuntimeExecutionInput,
    },
    infrastructure::providers::ollama::{OllamaProviderAdapter, OllamaProviderConfig},
};

fn start_server(
    status: &str,
    content_type: &str,
    body: String,
    delay: Duration,
    health_timeout: Duration,
) -> OllamaProviderAdapter {
    let listener = TcpListener::bind("127.0.0.1:0").expect("خادم الاختبار المحلي يجب أن يرتبط");
    let port = listener
        .local_addr()
        .expect("عنوان خادم الاختبار يجب أن يتاح")
        .port();
    let status = status.to_owned();
    let content_type = content_type.to_owned();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("طلب الاختبار يجب أن يصل");
        // استهلاك الطلب كاملاً قبل الرد: القراءة الواحدة قد تسبق اكتمال إرسال
        // العميل (طلبات chat أكبر) فيصل RST قبل جيل الاستجابة — مصدر رقّة
        // النقل الموثق؛ هذا الاستهلاك الكامل يحسمه حتمياً.
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
        thread::sleep(delay);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("استجابة الاختبار يجب أن تكتب");
    });

    let config = OllamaProviderConfig::for_loopback_port(port)
        .expect("منفذ الاختبار المحلي يجب أن يكون صالحاً");
    OllamaProviderAdapter::for_test(config, health_timeout, health_timeout)
}

#[test]
fn rejects_an_invalid_loopback_port() {
    assert!(matches!(
        OllamaProviderConfig::for_loopback_port(0),
        Err(ProviderFailure::Unavailable)
    ));
}

#[test]
fn reports_health_only_after_a_valid_tags_response() {
    let provider = start_server(
        "200 OK",
        "application/json",
        "{\"models\":[]}".to_owned(),
        Duration::ZERO,
        Duration::from_secs(1),
    );

    assert_eq!(provider.health(), ProviderHealth::Available);
}

#[test]
fn reports_unavailable_when_no_loopback_service_accepts_the_connection() {
    let config = OllamaProviderConfig::for_loopback_port(1).expect("المنفذ يجب أن يقبل في العقد");
    let provider = OllamaProviderAdapter::for_test(
        config,
        Duration::from_millis(50),
        Duration::from_millis(50),
    );

    assert_eq!(provider.health(), ProviderHealth::Unavailable);
}

#[test]
fn parses_realistic_model_listing_metadata() {
    let provider = start_server(
        "200 OK",
        "application/json",
        "{\"models\":[{\"name\":\"llama3.2:3b\",\"model\":\"llama3.2:3b\",\"modified_at\":\"2026-08-22T00:00:00Z\",\"size\":1234}]}".to_owned(),
        Duration::ZERO,
        Duration::from_secs(1),
    );

    let models = provider
        .discover_models()
        .expect("قائمة النماذج الصحيحة يجب أن تحلل");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id.as_str(), "llama3.2:3b");
    assert_eq!(models[0].size_bytes, Some(1234));
}

#[test]
fn rejects_malformed_or_non_json_model_listing() {
    let provider = start_server(
        "200 OK",
        "text/plain",
        "not-json".to_owned(),
        Duration::ZERO,
        Duration::from_secs(1),
    );

    assert!(matches!(
        provider.discover_models(),
        Err(ProviderFailure::InvalidResponse)
    ));
}

#[test]
fn rejects_model_listing_above_the_bounded_response_limit() {
    let oversized_body = format!("{{\"models\":[]}}{}", " ".repeat(256 * 1024));
    let provider = start_server(
        "200 OK",
        "application/json",
        oversized_body,
        Duration::ZERO,
        Duration::from_secs(1),
    );

    assert!(matches!(
        provider.discover_models(),
        Err(ProviderFailure::OutputTooLarge)
    ));
}

#[test]
fn reports_timeout_without_exposing_transport_detail() {
    let provider = start_server(
        "200 OK",
        "application/json",
        "{\"models\":[]}".to_owned(),
        Duration::from_millis(100),
        Duration::from_millis(10),
    );

    assert!(matches!(
        provider.discover_models(),
        Err(ProviderFailure::Timeout)
    ));
}

#[test]
fn accepts_a_non_streaming_generation_from_the_test_transport() {
    let provider = start_server(
        "200 OK",
        "application/json",
        "{\"model\":\"llama3.2:3b\",\"created_at\":\"2026-08-22T00:00:00Z\",\"response\":\"OK\",\"done\":true}".to_owned(),
        Duration::ZERO,
        Duration::from_secs(1),
    );
    let model_id = ModelId::new("llama3.2:3b").expect("معرف النموذج يجب أن يكون صالحاً");
    let input = RuntimeExecutionInput::new("Return the single word OK.")
        .expect("إدخال التشخيص يجب أن يكون صالحاً");

    let result = provider
        .generate_non_streaming(&model_id, &input)
        .expect("جيل اختبار النقل يجب أن ينجح");

    assert_eq!(result.output, "OK");
}

#[test]
fn rejects_tool_call_semantics_without_executing_them() {
    let provider = start_server(
        "200 OK",
        "application/json",
        "{\"model\":\"llama3.2:3b\",\"response\":\"ignored\",\"done\":true,\"tool_calls\":[]}"
            .to_owned(),
        Duration::ZERO,
        Duration::from_secs(1),
    );
    let model_id = ModelId::new("llama3.2:3b").expect("معرف النموذج يجب أن يكون صالحاً");
    let input = RuntimeExecutionInput::new("Return the single word OK.")
        .expect("إدخال التشخيص يجب أن يكون صالحاً");

    assert!(matches!(
        provider.generate_non_streaming(&model_id, &input),
        Err(ProviderFailure::ToolCallUnsupported)
    ));
}

fn start_chat_server(body: String) -> OllamaProviderAdapter {
    start_server(
        "200 OK",
        "application/json",
        body,
        Duration::ZERO,
        Duration::from_secs(2),
    )
}

fn chat_input() -> RuntimeExecutionInput {
    RuntimeExecutionInput::new("List obligations with the tool.")
        .expect("إدخال الاختبار يجب أن يكون صالحاً")
}

fn one_tool() -> [ProviderToolDefinition; 1] {
    [ProviderToolDefinition {
        name: "nexus_obligations_bounded_list_v1".to_owned(),
        description: "Reads a bounded list of obligations.".to_owned(),
        parameters_json: serde_json::json!({"type": "object", "properties": {}}).to_string(),
    }]
}

fn chat_body(message: serde_json::Value) -> String {
    serde_json::json!({
        "model": "qwen3:4b",
        "message": message,
        "done": true
    })
    .to_string()
}

#[test]
fn tool_aware_chat_path_returns_a_structured_tool_call_with_object_arguments() {
    let body = chat_body(serde_json::json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{"function": {"name": "nexus_obligations_bounded_list_v1", "arguments": {"limit": 5, "query": "privacy"}}}]
    }));
    let provider = start_chat_server(body);
    let model_id = ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً");
    match provider
        .generate_with_tools(&model_id, &chat_input(), &one_tool())
        .expect("الاستدعاء المهيكل يجب أن يُعاد")
    {
        ProviderToolResponse::ToolCall(call) => {
            assert_eq!(call.tool_name, "nexus_obligations_bounded_list_v1");
            assert_eq!(call.arguments["limit"], 5);
            assert_eq!(call.arguments["query"], "privacy");
        }
        ProviderToolResponse::Text(_) => panic!("يجب أن يكون الاستدعاء أداة لا نصاً"),
    }
}

#[test]
fn tool_aware_chat_path_accepts_string_arguments_as_json() {
    let body = chat_body(serde_json::json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{"function": {"name": "nexus_obligations_bounded_list_v1", "arguments": "{\"limit\":2}"}}]
    }));
    let provider = start_chat_server(body);
    let model_id = ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً");
    match provider
        .generate_with_tools(&model_id, &chat_input(), &one_tool())
        .expect("وسائط السلسلة يجب أن تُحلل")
    {
        ProviderToolResponse::ToolCall(call) => assert_eq!(call.arguments["limit"], 2),
        ProviderToolResponse::Text(_) => panic!("يجب أن يكون الاستدعاء أداة"),
    }
}

#[test]
fn tool_aware_chat_path_returns_text_when_the_model_answers_without_tools() {
    let body = chat_body(serde_json::json!({
        "role": "assistant",
        "content": "Bounded answer."
    }));
    let provider = start_chat_server(body);
    let model_id = ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً");
    match provider
        .generate_with_tools(&model_id, &chat_input(), &one_tool())
        .expect("النص النهائي يجب أن يُعاد")
    {
        ProviderToolResponse::Text(generation) => assert_eq!(generation.output, "Bounded answer."),
        ProviderToolResponse::ToolCall(_) => panic!("لا استدعاء أداة هنا"),
    }
}

#[test]
fn multiple_structured_tool_calls_in_one_response_fail_closed() {
    let body = chat_body(serde_json::json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [
            {"function": {"name": "a", "arguments": {}}},
            {"function": {"name": "b", "arguments": {}}}
        ]
    }));
    let provider = start_chat_server(body);
    let model_id = ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً");
    assert!(matches!(
        provider.generate_with_tools(&model_id, &chat_input(), &one_tool()),
        Err(ProviderFailure::ToolCallUnsupported)
    ));
}

#[test]
fn wrong_model_echo_in_tool_path_is_rejected() {
    let provider = start_chat_server(
        serde_json::json!({
            "model": "other:latest",
            "message": {"role": "assistant", "content": "x"},
            "done": true
        })
        .to_string(),
    );
    let model_id = ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً");
    assert!(matches!(
        provider.generate_with_tools(&model_id, &chat_input(), &one_tool()),
        Err(ProviderFailure::InvalidResponse)
    ));
}

#[test]
fn model_listing_reports_structured_tool_capability_deterministically() {
    let provider = start_server(
        "200 OK",
        "application/json",
        serde_json::json!({
            "models": [
                {"name": "qwen3:4b", "model": "qwen3:4b", "capabilities": ["completion", "tools", "thinking"]},
                {"name": "plain:latest", "model": "plain:latest", "capabilities": ["completion"]}
            ]
        })
        .to_string(),
        Duration::ZERO,
        Duration::from_secs(2),
    );
    let models = provider.discover_models().expect("الاستكشاف يجب أن ينجح");
    let qwen = models
        .iter()
        .find(|model| model.id.as_str() == "qwen3:4b")
        .expect("النموذج يجب أن يوجد");
    assert!(qwen.supports_tools);
    let plain = models
        .iter()
        .find(|model| model.id.as_str() == "plain:latest")
        .expect("النموذج الثاني يجب أن يوجد");
    assert!(!plain.supports_tools);
}
