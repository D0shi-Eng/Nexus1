use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::error::AppError;

// حدود الإدخال الحتمية (C-2) — لا إدخال غير محدود إطلاقاً
pub const MAX_SOURCE_TEXT_BYTES: usize = 64 * 1024;
pub const MIN_SOURCE_TEXT_CHARS: usize = 20;
pub const MAX_CHUNKS_PER_DOCUMENT: usize = 256;
pub const MAX_CHUNK_CHARS: usize = 1200;
pub const CHUNK_OVERLAP_CHARS: usize = 150;
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_ORIGIN_CHARS: usize = 200;
pub const MAX_FILENAME_CHARS: usize = 200;
pub const MAX_QUERY_CHARS: usize = 500;
pub const MAX_RETRIEVAL_ITEMS: usize = 5;
// أقصى محارف تُعرض/تُمرَّر للقطعة المسترجعة داخل السياق
pub const MAX_RETRIEVED_CHUNK_CONTEXT_CHARS: usize = 1500;
// نسبة محارف التحكم المسموحة قبل اعتبار المحتوى ثنائياً (مؤشر فحص لا ثقة عمياء)
const MAX_CONTROL_CHAR_RATIO: f64 = 0.02;

// الطوابع غير المنشأة إنتاجياً بعد (مصادر داخلية/مرجعية) جزء من عقد المخطط
// المغلق (C-1) — لا تُحذف، وقيود CHECK تحفظ نفس المجموعة في قاعدة البيانات.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeSourceType {
    UserProvidedDocument,
    NexusInternalRecord,
    RegulatorySourceReference,
}

impl KnowledgeSourceType {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::UserProvidedDocument => "USER_PROVIDED_DOCUMENT",
            Self::NexusInternalRecord => "NEXUS_INTERNAL_RECORD",
            Self::RegulatorySourceReference => "REGULATORY_SOURCE_REFERENCE",
        }
    }
}

// قيم المنشأ الكاملة جزء من العقد (C-1) — الكاتب المنتجي الحالي إدخال المستخدم فقط.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeProvenance {
    UserProvided,
    NexusInternal,
    DemoSynthetic,
    ExternalReference,
    Unknown,
}

impl KnowledgeProvenance {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::UserProvided => "USER_PROVIDED",
            Self::NexusInternal => "NEXUS_INTERNAL",
            Self::DemoSynthetic => "DEMO_SYNTHETIC",
            Self::ExternalReference => "EXTERNAL_REFERENCE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeSourceStatus {
    Active,
    Disabled,
    Failed,
}

impl KnowledgeSourceStatus {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Disabled => "DISABLED",
            Self::Failed => "FAILED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "ACTIVE" => Ok(Self::Active),
            "DISABLED" => Ok(Self::Disabled),
            "FAILED" => Ok(Self::Failed),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeDocumentFormat {
    Txt,
    Markdown,
}

impl KnowledgeDocumentFormat {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Txt => "TXT",
            Self::Markdown => "MARKDOWN",
        }
    }

    // الصيغة تُستنتج من الامتداد كمؤشر فقط — القبول النهائي على المحتوى نفسه (§17)
    pub fn from_filename_hint(filename: &str) -> Option<Self> {
        let normalized = filename.trim().to_ascii_lowercase();
        if normalized.ends_with(".md") || normalized.ends_with(".markdown") {
            Some(Self::Markdown)
        } else if normalized.ends_with(".txt") {
            Some(Self::Txt)
        } else {
            None
        }
    }
}

pub fn content_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

// فحص المحتوى: نص حقيقي لا ثنائيات — لا نثق بالاسم/الامتداد وحده (§17/C-2)
pub fn validate_source_text(text: &str) -> Result<(), AppError> {
    let char_count = text.chars().count();
    if char_count < MIN_SOURCE_TEXT_CHARS {
        return Err(AppError::KnowledgeSourceInvalid);
    }
    if text.as_bytes().len() > MAX_SOURCE_TEXT_BYTES {
        return Err(AppError::KnowledgeSourceTooLarge);
    }
    let control_count = text
        .chars()
        .filter(|character| {
            character.is_control() && *character != '\n' && *character != '\r' && *character != '\t'
        })
        .count();
    let ratio = control_count as f64 / char_count as f64;
    if ratio > MAX_CONTROL_CHAR_RATIO {
        return Err(AppError::KnowledgeSourceInvalid);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeChunkPlan {
    pub chunk_index: usize,
    pub text: String,
    pub text_hash: String,
    pub char_start: usize,
    pub char_end: usize,
}

// تقسيم حتمي مستقر: نوافذ بحد أقصى محارف مع تراكب ثابت، والحدود تُزاح إلى
// أقرب فاصل أسطر/مسافة داخل نافذة الإزاحة — إعادة البناء تعطي نفس الناتج
// دائماً لنفس المدخل (لا قرار نموذج في الحدود §20/C-2).
pub fn chunk_deterministic(text: &str) -> Result<Vec<KnowledgeChunkPlan>, AppError> {
    let normalized = text.trim();
    if normalized.is_empty() {
        return Err(AppError::KnowledgeSourceInvalid);
    }
    if normalized.chars().count() < MIN_SOURCE_TEXT_CHARS {
        return Err(AppError::KnowledgeSourceInvalid);
    }

    let chars: Vec<char> = normalized.chars().collect();
    let total = chars.len();
    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < total {
        if chunks.len() >= MAX_CHUNKS_PER_DOCUMENT {
            return Err(AppError::KnowledgeSourceTooLarge);
        }
        let hard_end = (start + MAX_CHUNK_CHARS).min(total);
        if hard_end == total {
            let chunk_text: String = chars[start..total].iter().collect();
            push_chunk(&mut chunks, start, total, &chunk_text)?;
            break;
        }

        // إزاحة الحد إلى أقرب فاصل داخل آخر 150 محرفاً من النافذة
        let window_start = (hard_end - CHUNK_OVERLAP_CHARS).max(start + 1);
        let mut cut = None;
        for position in (window_start..hard_end).rev() {
            if chars[position] == '\n' || chars[position] == ' ' {
                cut = Some(position);
                break;
            }
        }
        let end = cut.unwrap_or(hard_end);
        let chunk_text: String = chars[start..end].iter().collect();
        push_chunk(&mut chunks, start, end, &chunk_text)?;

        // الخطوة التالية تبدأ بعد الفاصل (تخطي الفواصل المتتالية)
        let mut next = end;
        while next < total && (chars[next] == '\n' || chars[next] == ' ') {
            next += 1;
        }
        // ضمان تقدم حتمي: لا نافذة أصغر من محرف واحد
        start = next.max(end + 1);
    }

    Ok(chunks)
}

fn push_chunk(
    chunks: &mut Vec<KnowledgeChunkPlan>,
    start: usize,
    end: usize,
    text: &str,
) -> Result<(), AppError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        // نافذة بيضاء بالكامل تُتجاهل حتمياً — لا قطع فارغة
        return Ok(());
    }
    chunks.push(KnowledgeChunkPlan {
        chunk_index: chunks.len(),
        text: trimmed.to_owned(),
        text_hash: content_hash(trimmed),
        char_start: start,
        char_end: end,
    });
    Ok(())
}

// عنصر استرجاع مطبوع (§27) — لا كائنات قاعدة تمر للنموذج مباشرة
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRetrievalItem {
    pub source_id: String,
    pub document_id: String,
    pub chunk_id: String,
    pub source_title: String,
    pub provenance: String,
    pub rank: u32,
    pub score: f64,
    pub content: String,
}

// عقد الاقتباس المطبوع (C-6) — يُبنى خلفياً من القطع المسترجعة الفعلية فقط
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCitation {
    pub citation_id: String,
    pub source_id: String,
    pub document_id: String,
    pub chunk_id: String,
    pub display_label: String,
    pub provenance: String,
    pub retrieved_rank: u32,
}

impl KnowledgeCitation {
    pub fn from_retrieval_item(item: &KnowledgeRetrievalItem) -> Self {
        Self {
            citation_id: format!("CIT-{}", item.rank + 1),
            source_id: item.source_id.clone(),
            document_id: item.document_id.clone(),
            chunk_id: item.chunk_id.clone(),
            display_label: item.source_title.clone(),
            provenance: item.provenance.clone(),
            retrieved_rank: item.rank,
        }
    }
}

// تطبيع استعلام FTS: كل رمز داخل علامات اقتباس — يمنع حقن صيغة FTS5
pub fn sanitize_fts_query(query: &str) -> Option<String> {
    if query.trim().is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return None;
    }
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|character| *character != '"')
                .collect::<String>()
        })
        .filter(|token| token.chars().count() >= 2)
        .map(|token| format!("\"{token}\""))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        // صلة OR: استدعاء معجمي مع ترتيب bm25 — لا مطابقة كلية قسرية
        Some(tokens.join(" OR "))
    }
}

// استخراج مراجع الاقتباس من نص النموذج حتمياً (CIT-n فقط)
pub fn extract_citation_references(output: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut remainder = output;
    while let Some(position) = remainder.find("CIT-") {
        let after = &remainder[position + 4..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && digits.len() <= 4 {
            let reference = format!("CIT-{digits}");
            if !references.contains(&reference) {
                references.push(reference);
            }
        }
        remainder = &remainder[position + 4 + digits.len()..];
    }
    references
}

#[cfg(test)]
mod tests {
    use super::{
        chunk_deterministic, content_hash, extract_citation_references, sanitize_fts_query,
        validate_source_text, KnowledgeDocumentFormat,
    };

    #[test]
    fn chunking_is_deterministic_and_bounded() {
        let text = "العربية والإنجليزية معاً ".repeat(60);
        let first = chunk_deterministic(&text).expect("التقسيم يجب أن ينجح");
        let second = chunk_deterministic(&text).expect("الإعادة يجب أن تطابق");
        assert_eq!(first, second);
        assert!(first.len() > 1);
        for chunk in &first {
            assert!(chunk.text.chars().count() <= 1200);
            assert!(!chunk.text.trim().is_empty());
        }
    }

    #[test]
    fn chunk_indices_are_sequential_and_hashed() {
        let text = "first paragraph with enough content to pass validation.\n\nsecond paragraph follows here with more words.";
        let chunks = chunk_deterministic(text).expect("التقسيم يجب أن ينجح");
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, index);
            assert_eq!(chunk.text_hash, content_hash(&chunk.text));
            assert!(chunk.char_end > chunk.char_start);
        }
    }

    #[test]
    fn oversized_text_is_rejected_before_chunking() {
        let big = "a".repeat(64 * 1024 + 100);
        assert!(validate_source_text(&big).is_err());
    }

    #[test]
    fn binary_like_content_is_rejected_by_content_inspection() {
        let binaryish = format!("{}\u{0000}\u{0001}", "text ".repeat(10));
        assert!(validate_source_text(&binaryish).is_err());
    }

    #[test]
    fn too_short_text_is_rejected() {
        assert!(validate_source_text("قصير").is_err());
    }

    #[test]
    fn filename_hint_only_accepts_bounded_text_formats() {
        assert_eq!(
            KnowledgeDocumentFormat::from_filename_hint("notes.md"),
            Some(KnowledgeDocumentFormat::Markdown)
        );
        assert_eq!(
            KnowledgeDocumentFormat::from_filename_hint("report.txt"),
            Some(KnowledgeDocumentFormat::Txt)
        );
        assert_eq!(KnowledgeDocumentFormat::from_filename_hint("doc.pdf"), None);
        assert_eq!(
            KnowledgeDocumentFormat::from_filename_hint("shell.ps1"),
            None
        );
    }

    #[test]
    fn fts_query_is_quoted_and_sanitized() {
        assert_eq!(
            sanitize_fts_query(" Obligation \"quote\" "),
            Some("\"Obligation\" OR \"quote\"".to_owned())
        );
        assert_eq!(sanitize_fts_query("  "), None);
        assert_eq!(sanitize_fts_query("a b"), None);
        assert_eq!(
            sanitize_fts_query("DELETE FROM x; الانظمة"),
            Some("\"DELETE\" OR \"FROM\" OR \"x;\" OR \"الانظمة\"".to_owned())
        );
    }

    #[test]
    fn citation_extraction_is_deterministic_and_deduplicated() {
        let references =
            extract_citation_references("answer [CIT-1] and (CIT-3) plus CIT-1 again and CITITY");
        assert_eq!(references, vec!["CIT-1", "CIT-3"]);
    }
}
