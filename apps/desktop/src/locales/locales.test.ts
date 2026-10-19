import { describe, expect, it } from 'vitest';
import { safeErrorCodes } from '@nexus/contracts';
import { ar } from './ar';
import { en } from './en';

type TranslationNode = string | { readonly [key: string]: TranslationNode };

const collectKeys = (node: TranslationNode, prefix = ''): string[] =>
  Object.entries(node).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof value === 'string' ? [path] : collectKeys(value, path);
  });

// مفاتيح رموز الأخطاء الآمنة تُكتب دائماً بالشكل العلوي — ما عدا ذلك رسائل عرضية لا رموز.
const ERROR_CODE_PATTERN = /^[A-Z][A-Z0-9_]*$/;

describe('translation resources', () => {
  it('keep Arabic and English keys aligned', () => {
    expect(collectKeys(ar)).toEqual(collectKeys(en));
  });

  it('translate every frontend-visible safe error code in both languages', () => {
    for (const code of safeErrorCodes) {
      expect(typeof ar.errors[code]).toBe('string');
      expect(typeof en.errors[code]).toBe('string');
    }
  });

  // الاتجاه المعاكس: كل مفتاح رمزي تحت errors يجب أن يكون في كتالوج الرموز الآمنة — يكشف الرموز المتقادمة.
  it('keeps every errors-section code registered in the safe error catalog', () => {
    const arErrorCodes = Object.keys(ar.errors).filter((key) => ERROR_CODE_PATTERN.test(key));
    const enErrorCodes = Object.keys(en.errors).filter((key) => ERROR_CODE_PATTERN.test(key));
    expect(arErrorCodes).toEqual(enErrorCodes);
    for (const key of arErrorCodes) {
      expect(safeErrorCodes).toContain(key);
    }
  });

  // انحدار الإصدار: تبادل القيم بين اللغتين لا يكشفه تطابق المفاتيح إطلاقاً.
  // رسائل الأخطاء تُعرض للمستخدم عبر t(`errors.CODE`) — فيجب أن تكون بالعربية في ar وبالإنجليزية في en.
  it('never leaves an error message in the wrong language', () => {
    const ARABIC_SCRIPT = /[\u0621-\u064A]/;
    const LATIN_SCRIPT = /[A-Za-z]/;

    const misplacedInArabic = Object.entries(ar.errors)
      .filter(([key]) => ERROR_CODE_PATTERN.test(key))
      .filter(([, value]) => !ARABIC_SCRIPT.test(value));
    expect(misplacedInArabic).toEqual([]);

    // الرسائل العربية خارج قسم errors مسموح بها (اسم المنتج/اسم اللغة)، لكن قسم errors لا يقبلها.
    const misplacedInEnglish = Object.entries(en.errors)
      .filter(([key]) => ERROR_CODE_PATTERN.test(key))
      .filter(([, value]) => !LATIN_SCRIPT.test(value));
    expect(misplacedInEnglish).toEqual([]);
  });
});
