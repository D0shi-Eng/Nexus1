import { describe, expect, it } from 'vitest';
import { applyDocumentLanguage } from './i18n';

describe('document language direction', () => {
  it('applies RTL for Arabic and LTR for English', () => {
    applyDocumentLanguage('ar');
    expect(document.documentElement.lang).toBe('ar');
    expect(document.documentElement.dir).toBe('rtl');

    applyDocumentLanguage('en');
    expect(document.documentElement.lang).toBe('en');
    expect(document.documentElement.dir).toBe('ltr');
  });
});
