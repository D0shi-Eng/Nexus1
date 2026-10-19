UPDATE regulatory_sources
SET official = 0,
    verification_state = 'REQUIRES_REVIEW',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE authority LIKE 'DEMO / SYNTHETIC%'
   OR source_url LIKE 'https://example.invalid/demo/%';

-- لا توجد في هذا الإصدار مساحة تخزين ملفات أو فحص محتوى؛ لذلك لا يجوز إبقاء أي مرجع مسجل بحالة «مرفوع».
UPDATE evidence_items
SET status = 'REQUIRES_REVIEW',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status = 'UPLOADED';
