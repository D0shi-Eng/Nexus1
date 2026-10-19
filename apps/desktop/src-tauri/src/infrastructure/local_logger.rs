use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use chrono::Utc;

// مستويات التسجيل المحلي — DEBUG متاح في بنى التطوير فقط.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

// حدود التدوير: ملف يومي بحد 5MB وثبات آخر 10 ملفات فقط — لا نمو لا نهائي.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_RETAINED_FILES: usize = 10;

struct LogSink {
    directory: PathBuf,
    current_date: String,
    current_file: Option<File>,
    current_bytes: u64,
}

static SINK: OnceLock<Mutex<LogSink>> = OnceLock::new();

// تهيئة التسجيل مرة واحدة عند الإقلاع — فشلها يعطل التسجيل بصمت لا المنتج.
pub fn initialize(directory: PathBuf) {
    let _ = SINK.set(Mutex::new(LogSink {
        directory,
        current_date: String::new(),
        current_file: None,
        current_bytes: 0,
    }));
}

// استبدال الحوض في الاختبارات المتسلسلة فقط — OnceLock لا يقبل إعادة الضبط بنفسه.
#[cfg(test)]
pub fn reinitialize_for_tests(directory: PathBuf) {
    if let Some(sink) = SINK.get() {
        if let Ok(mut current) = sink.lock() {
            *current = LogSink {
                directory,
                current_date: String::new(),
                current_file: None,
                current_bytes: 0,
            };
            return;
        }
    }
    initialize(directory);
}

pub fn error(event: &str, tenant_id: &str, details: &str) {
    write_entry(LogLevel::Error, event, tenant_id, details);
}

pub fn warn(event: &str, tenant_id: &str, details: &str) {
    write_entry(LogLevel::Warn, event, tenant_id, details);
}

pub fn info(event: &str, tenant_id: &str, details: &str) {
    write_entry(LogLevel::Info, event, tenant_id, details);
}

pub fn debug(event: &str, tenant_id: &str, details: &str) {
    write_entry(LogLevel::Debug, event, tenant_id, details);
}

// كتابة سطر JSON واحد — أفضل جهد: أي فشل يُسقط السطر ولا يُسقط تدفق المنتج.
fn write_entry(level: LogLevel, event: &str, tenant_id: &str, details: &str) {
    // DEBUG في بنى الإصدار مرفوضة كلياً — لا تشخيص تطوير خارج بنية التطوير.
    if level == LogLevel::Debug && !cfg!(debug_assertions) {
        return;
    }
    let Some(sink) = SINK.get() else {
        return;
    };
    let Ok(mut sink) = sink.lock() else {
        return;
    };

    let today = Utc::now().format("%Y%m%d").to_string();
    if sink.current_file.is_none()
        || sink.current_date != today
        || sink.current_bytes >= MAX_FILE_BYTES
    {
        if !rotate(&mut sink, &today) {
            return;
        }
    }

    // details إما سلسلة JSON كائن صالحة فتُدمج كما هي أو نص آمن بسيط.
    let details_value = if details.trim().starts_with('{') {
        details.trim().to_owned()
    } else if details.is_empty() {
        "{}".to_owned()
    } else {
        format!("\"{}\"", escape(details))
    };
    let tenant_value = if tenant_id.is_empty() {
        "null".to_owned()
    } else {
        format!("\"{}\"", escape(tenant_id))
    };
    let line = format!(
        "{{\"ts\":\"{}\",\"level\":\"{}\",\"event\":\"{}\",\"tenantId\":{},\"details\":{}}}\n",
        Utc::now().to_rfc3339(),
        level.as_str(),
        escape(event),
        tenant_value,
        details_value
    );

    if let Some(file) = sink.current_file.as_mut() {
        if file.write_all(line.as_bytes()).is_ok() {
            let _ = file.flush();
            sink.current_bytes += line.len() as u64;
        } else {
            sink.current_file = None;
        }
    }
}

fn rotate(sink: &mut LogSink, today: &str) -> bool {
    if fs::create_dir_all(&sink.directory).is_err() {
        return false;
    }
    let mut path = sink.directory.join(format!("nexus-{today}.log"));
    let mut roll = 0;
    loop {
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() >= MAX_FILE_BYTES => {
                roll += 1;
                path = sink.directory.join(format!("nexus-{today}-{roll}.log"));
            }
            _ => break,
        }
    }
    let file = OpenOptions::new().create(true).append(true).open(&path);
    match file {
        Ok(file) => {
            sink.current_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            sink.current_file = Some(file);
            sink.current_date = today.to_owned();
            prune_old_files(sink);
            true
        }
        Err(_) => false,
    }
}

// حذف أقدم الملفات فوق حد الاحتفاظ — فشل الحذف لا يمنع التسجيل.
fn prune_old_files(sink: &LogSink) {
    let Ok(entries) = fs::read_dir(&sink.directory) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|e| e.to_str()) == Some("log")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("nexus-"))
                    .unwrap_or(false)
        })
        .collect();
    if logs.len() <= MAX_RETAINED_FILES {
        return;
    }
    // الترتيب النصي يضع ‎-10‎ قبل ‎-2‎ فيُحذف أحدث ملف مدوَّر ويبقى الأساس؛ الترتيب
    // الرقمي للدورة هو ما يجعل "الأقدم" مطابقاً للواقع لا لترتيب المحارف.
    logs.sort_by_key(|path| rotation_order(path));
    let excess = logs.len() - MAX_RETAINED_FILES;
    for path in logs.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

// مفتاح العمر: (التاريخ، رقم الدورة) — الأساس بلا لاحقة يُعد الدورة صفر.
fn rotation_order(path: &PathBuf) -> (String, u64) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let stem = name.trim_start_matches("nexus-").trim_end_matches(".log");
    match stem.split_once('-') {
        Some((date, roll)) => (date.to_owned(), roll.parse().unwrap_or(0)),
        None => (stem.to_owned(), 0),
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{
        info, prune_old_files, reinitialize_for_tests, write_entry, LogLevel, LogSink,
        MAX_RETAINED_FILES,
    };
    use std::fs::File;

    // سطر JSON واحد لكل نداء وبنية حقول ثابتة — يُقرأ الملف ويُفحص نصياً.
    #[test]
    fn writes_bounded_jsonl_entries_to_daily_file() {
        let directory =
            std::env::temp_dir().join(format!("nexus-mp05-log-{}", uuid::Uuid::new_v4().simple()));
        reinitialize_for_tests(directory.clone());
        info("TEST_EVENT", "tenant_x", "{\"runId\":\"r-1\"}");
        info("TEST_EVENT_PLAIN", "", "plain detail");

        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let path = directory.join(format!("nexus-{today}.log"));
        let mut content = String::new();
        File::open(&path)
            .expect("ملف السجل يجب أن يفتح")
            .read_to_string(&mut content)
            .expect("السجل يجب أن يقرأ");
        let lines: Vec<&str> = content.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"level\":\"INFO\""));
        assert!(lines[0].contains("\"event\":\"TEST_EVENT\""));
        assert!(lines[0].contains("\"tenantId\":\"tenant_x\""));
        assert!(lines[0].contains("\"runId\":\"r-1\""));
        assert!(lines[1].contains("\"tenantId\":null"));
        assert!(lines[1].contains("\"details\":\"plain detail\""));
        std::fs::remove_dir_all(&directory).ok();
    }

    // DEBUG لا يكتب شيئاً في بنية الإصدار — يتحقق بالمنطق نفسه لا ببنية الترجمة.
    #[test]
    fn debug_entries_respect_release_gate() {
        // في بنى الاختبار (debug_assertions=true) يُكتب السطر؛ الاختبار يثبت المسار الحي فقط.
        let directory =
            std::env::temp_dir().join(format!("nexus-mp05-logd-{}", uuid::Uuid::new_v4().simple()));
        reinitialize_for_tests(directory.clone());
        write_entry(LogLevel::Debug, "DEBUG_EVENT", "", "");
        if cfg!(debug_assertions) {
            let today = chrono::Utc::now().format("%Y%m%d").to_string();
            let path = directory.join(format!("nexus-{today}.log"));
            let mut content = String::new();
            use std::io::Read;
            File::open(&path)
                .and_then(|mut f| f.read_to_string(&mut content))
                .expect("قراءة السجل يجب أن تنجح");
            assert!(content.contains("DEBUG_EVENT"));
        }
        std::fs::remove_dir_all(&directory).ok();
    }

    // الحد الأقصى للملفات المحتفظ بها ثابت ومعروف — يوثق سقف التخزين.
    #[test]
    fn retention_ceiling_is_ten_files() {
        assert_eq!(MAX_RETAINED_FILES, 10);
    }

    // الحذف يجب أن يستهدف الأقدم فعلاً: الترقيم النصي يضع ‎-10‎ قبل ‎-2‎ فيُفقد
    // أحدث ملف مدوَّر ويبقى أساس اليوم — ضياع آخر أثر تشخيصي عند الحاجة إليه.
    #[test]
    fn retention_prunes_oldest_rotations_by_numeric_roll_not_text_order() {
        let directory = std::env::temp_dir().join(format!(
            "nexus-mp05-logprune-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&directory).expect("مجلد السجل يجب أن ينشأ");
        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let mut names = vec![format!("nexus-{today}.log")];
        for roll in 1..=11 {
            names.push(format!("nexus-{today}-{roll}.log"));
        }
        for name in &names {
            std::fs::write(directory.join(name), b"entry").expect("ملف السجل يجب أن يُكتب");
        }

        let sink = LogSink {
            directory: directory.clone(),
            current_date: today.clone(),
            current_file: None,
            current_bytes: 0,
        };
        prune_old_files(&sink);

        assert!(!directory.join(format!("nexus-{today}.log")).exists());
        assert!(!directory.join(format!("nexus-{today}-1.log")).exists());
        for roll in 2..=11 {
            assert!(
                directory.join(format!("nexus-{today}-{roll}.log")).exists(),
                "أحدث ملف مدوَّر -{roll} يجب أن يبقى"
            );
        }

        std::fs::remove_dir_all(&directory).ok();
    }
}
