// مولّد طبقة رموز التصميم — مصدر واحد لكل الثيمات.
//
// سبب وجود المولّد: كل ثيم يجب أن يعلن **المجموعة الكاملة نفسها** من الرموز
// الدلالية. كتابة ستّ لوحات كاملة يدوياً تُنتج انحرافاً حتمياً (رمز ناقص في ثيم
// ما فيرث قيمة ثيم آخر داخل المعاينة). هنا تُعرَّف اللوحات مرة واحدة، ويُولَّد
// الملف، ويُثبت اختبار `design-tokens.test.ts` اكتمال كل كتلة.
//
// التشغيل: node tools/theme/generate-tokens.mjs
import { writeFile } from 'node:fs/promises';

const OUT = 'D:/NEXUS/apps/desktop/src/styles/tokens.css';

// ===== الرموز البنيوية: لا تتغيّر بالثيم، فتُعلن مرة واحدة على :root =====
const structural = {
  '--font-sans': "'IBM Plex Sans Arabic', 'Segoe UI Variable Text', 'Segoe UI', Tahoma, 'Noto Sans Arabic', system-ui, -apple-system, sans-serif",
  '--font-display': "'IBM Plex Sans Arabic', 'Segoe UI Variable Display', 'Segoe UI', Tahoma, system-ui, sans-serif",
  '--font-mono': "'Cascadia Mono', 'Consolas', 'SFMono-Regular', 'Courier New', monospace",
  '--weight-regular': '400',
  '--weight-medium': '500',
  '--weight-semibold': '600',
  '--weight-bold': '700',
  '--fs-display': '1.75rem',
  '--fs-h1': '1.5rem',
  '--fs-h2': '1.0625rem',
  '--fs-h3': '0.9375rem',
  '--fs-body': '0.875rem',
  '--fs-body-sm': '0.8125rem',
  '--fs-label': '0.75rem',
  '--fs-caption': '0.6875rem',
  '--fs-table': '0.8125rem',
  '--fs-eyebrow': '0.6875rem',
  '--lh-tight': '1.25',
  '--lh-heading': '1.35',
  '--lh-body': '1.6',
  '--ls-tight': '-0.02em',
  '--ls-normal': '0',
  '--ls-wide': '0.06em',
  '--space-0': '0',
  '--space-1': '4px',
  '--space-2': '8px',
  '--space-3': '12px',
  '--space-4': '16px',
  '--space-5': '20px',
  '--space-6': '24px',
  '--space-7': '32px',
  '--space-8': '40px',
  '--space-9': '48px',
  '--radius-xs': '4px',
  '--radius-sm': '6px',
  '--radius-md': '8px',
  '--radius-lg': '10px',
  '--radius-xl': '14px',
  '--radius-pill': '999px',
  '--duration-fast': '120ms',
  '--duration-base': '180ms',
  '--easing-standard': 'cubic-bezier(0.2, 0, 0.2, 1)',
  '--z-sidebar': '30',
  '--z-topbar': '20',
  '--z-backdrop': '50',
  '--z-dialog': '80',
  '--z-feedback': '70',
  '--sidebar-width': '268px',
  '--topbar-height': '62px',
  '--content-max': '1480px',
  '--focus-ring-offset': '2px',
  '--focus-ring-width': '3px',
  '--focus-ring': 'var(--focus-ring-width) solid var(--focus-ring-color)',
};

// ===== اللوحات الدلالية: كل ثيم يعلن هذه المجموعة كاملةً =====
const themes = {
  // ---------- فاتح (الافتراضي) ----------
  light: {
    scheme: 'light',
    tokens: {
      '--surface-page': '#f4f6fa',
      '--surface-card': '#ffffff',
      '--surface-elevated': '#fbfcfe',
      '--surface-muted': '#f1f5f9',
      '--surface-inset': '#f7f9fc',
      '--surface-strong': '#e8f0f9',
      '--surface-hover': '#f5f8fb',
      '--surface-overlay': 'rgba(9, 21, 38, 0.5)',
      '--surface-navy': '#13233a',
      '--surface-navy-raised': '#1b2f4b',
      '--surface-navy-active': '#22405f',
      '--surface-navy-border': '#24374f',
      '--text-on-dark': '#d6e0ee',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#9dadc2',
      '--border': '#dce4ec',
      '--border-subtle': '#e9eff5',
      '--border-strong': '#c3d0de',
      '--text-strong': '#16202f',
      '--text-base': '#2b3a4d',
      '--text-muted': '#5c6b7e',
      '--text-subtle': '#7e8b9c',
      '--text-inverse': '#ffffff',
      '--accent': '#255e96',
      '--accent-strong': '#1c4a78',
      '--accent-soft': '#e8f0f9',
      '--accent-contrast': '#ffffff',
      '--brand-navy': '#14263f',
      '--brand-gold': '#c9a24a',
      '--brand-gold-deep': '#a9832f',
      '--status-success': '#1c6f4c',
      '--status-success-soft': '#e7f4ed',
      '--status-success-border': '#bfe0cf',
      '--status-danger': '#ab353c',
      '--status-danger-soft': '#fdedee',
      '--status-danger-border': '#f0cccf',
      '--status-warning': '#8a6412',
      '--status-warning-soft': '#fdf6e4',
      '--status-warning-border': '#efdcaa',
      '--status-info': '#2a6ba1',
      '--status-info-soft': '#e9f1f9',
      '--status-info-border': '#c6dcef',
      '--status-neutral': '#54677d',
      '--status-neutral-soft': '#eef2f6',
      '--status-neutral-border': '#dbe3ea',
      '--shadow-xs': '0 1px 2px rgba(16, 32, 54, 0.05)',
      '--shadow-sm': '0 1px 3px rgba(16, 32, 54, 0.08), 0 1px 2px rgba(16, 32, 54, 0.04)',
      '--shadow-lg': '0 18px 60px rgba(9, 24, 44, 0.26)',
      '--focus-ring-color': '#2f6fb0',
    },
  },

  // ---------- داكن ----------
  dark: {
    scheme: 'dark',
    tokens: {
      '--surface-page': '#0e141c',
      '--surface-card': '#161e28',
      '--surface-elevated': '#1a2330',
      '--surface-muted': '#1d2734',
      '--surface-inset': '#131b24',
      '--surface-strong': '#1d2c3e',
      '--surface-hover': '#1b2533',
      '--surface-overlay': 'rgba(3, 8, 14, 0.66)',
      '--surface-navy': '#0b131e',
      '--surface-navy-raised': '#16222f',
      '--surface-navy-active': '#1f3247',
      '--surface-navy-border': '#1e2b3a',
      '--text-on-dark': '#cdd9e8',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#8b9bad',
      '--border': '#2b3746',
      '--border-subtle': '#222e3c',
      '--border-strong': '#3c4c60',
      '--text-strong': '#e7eef7',
      '--text-base': '#c8d4e2',
      '--text-muted': '#9aa8ba',
      '--text-subtle': '#7d8b9c',
      '--text-inverse': '#0e141c',
      '--accent': '#4f9ade',
      '--accent-strong': '#74b4ec',
      '--accent-soft': '#1a2a3c',
      '--accent-contrast': '#08131f',
      '--brand-navy': '#0b131e',
      '--brand-gold': '#dcb96c',
      '--brand-gold-deep': '#c19b46',
      '--status-success': '#56c391',
      '--status-success-soft': '#122c22',
      '--status-success-border': '#23503c',
      '--status-danger': '#f08a90',
      '--status-danger-soft': '#2e1719',
      '--status-danger-border': '#56282c',
      '--status-warning': '#dfbc63',
      '--status-warning-soft': '#2c2513',
      '--status-warning-border': '#54471f',
      '--status-info': '#74b4ec',
      '--status-info-soft': '#14273a',
      '--status-info-border': '#274763',
      '--status-neutral': '#a3b1c2',
      '--status-neutral-soft': '#1e2833',
      '--status-neutral-border': '#313e4d',
      '--shadow-xs': '0 1px 2px rgba(0, 0, 0, 0.4)',
      '--shadow-sm': '0 1px 3px rgba(0, 0, 0, 0.45), 0 1px 2px rgba(0, 0, 0, 0.3)',
      '--shadow-lg': '0 18px 60px rgba(0, 0, 0, 0.55)',
      '--focus-ring-color': '#7ab6ee',
    },
  },

  // ---------- منتصف الليل: نيلي/بنفسجي داكن ----------
  midnight: {
    scheme: 'dark',
    tokens: {
      '--surface-page': '#0d0b1a',
      '--surface-card': '#15122a',
      '--surface-elevated': '#1a1633',
      '--surface-muted': '#1e1a38',
      '--surface-inset': '#120f22',
      '--surface-strong': '#241d47',
      '--surface-hover': '#1c1735',
      '--surface-overlay': 'rgba(5, 3, 14, 0.72)',
      '--surface-navy': '#0a0817',
      '--surface-navy-raised': '#171331',
      '--surface-navy-active': '#241a52',
      '--surface-navy-border': '#241d47',
      '--text-on-dark': '#d9d2f5',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#9a92c6',
      '--border': '#2f2854',
      '--border-subtle': '#262045',
      '--border-strong': '#4a3f80',
      '--text-strong': '#ece9ff',
      '--text-base': '#c9c2ec',
      '--text-muted': '#9b93c4',
      '--text-subtle': '#7d76a3',
      '--text-inverse': '#0d0b1a',
      '--accent': '#8b7bf0',
      '--accent-strong': '#a89bff',
      '--accent-soft': '#241d47',
      '--accent-contrast': '#0b0918',
      '--brand-navy': '#0a0817',
      '--brand-gold': '#e8c07a',
      '--brand-gold-deep': '#c79c4e',
      '--status-success': '#5fd3a5',
      '--status-success-soft': '#122a22',
      '--status-success-border': '#255a45',
      '--status-danger': '#f58a9c',
      '--status-danger-soft': '#30161d',
      '--status-danger-border': '#5c2a37',
      '--status-warning': '#e8c07a',
      '--status-warning-soft': '#2c2413',
      '--status-warning-border': '#54431f',
      '--status-info': '#8bb8f5',
      '--status-info-soft': '#16233a',
      '--status-info-border': '#2a4468',
      '--status-neutral': '#b0a8d8',
      '--status-neutral-soft': '#1f1a35',
      '--status-neutral-border': '#332c55',
      '--shadow-xs': '0 1px 2px rgba(3, 1, 12, 0.5)',
      '--shadow-sm': '0 1px 3px rgba(3, 1, 12, 0.55), 0 1px 2px rgba(3, 1, 12, 0.35)',
      '--shadow-lg': '0 18px 60px rgba(3, 1, 12, 0.7)',
      '--focus-ring-color': '#b3a6ff',
    },
  },

  // ---------- رملي: فاتح دافئ بلمسة زمردية مزرقّة ----------
  sand: {
    scheme: 'light',
    tokens: {
      '--surface-page': '#faf7f2',
      '--surface-card': '#ffffff',
      '--surface-elevated': '#fdfbf8',
      '--surface-muted': '#f4efe7',
      '--surface-inset': '#f8f4ee',
      '--surface-strong': '#efe6d8',
      '--surface-hover': '#f6f1e9',
      '--surface-overlay': 'rgba(40, 30, 15, 0.45)',
      '--surface-navy': '#1f2b2a',
      '--surface-navy-raised': '#2a3937',
      '--surface-navy-active': '#354a47',
      '--surface-navy-border': '#33423f',
      '--text-on-dark': '#d8e4e0',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#a2b3ad',
      '--border': '#e3d9c9',
      '--border-subtle': '#ece4d7',
      '--border-strong': '#cdbfa8',
      '--text-strong': '#2b2418',
      '--text-base': '#4a3f2e',
      '--text-muted': '#6f6250',
      '--text-subtle': '#8d8070',
      '--text-inverse': '#ffffff',
      '--accent': '#0f7b6c',
      '--accent-strong': '#0b6355',
      '--accent-soft': '#e3f2ef',
      '--accent-contrast': '#ffffff',
      '--brand-navy': '#1c2b33',
      '--brand-gold': '#b8862f',
      '--brand-gold-deep': '#97692a',
      '--status-success': '#1a7a52',
      '--status-success-soft': '#e6f4ec',
      '--status-success-border': '#bfe0cf',
      '--status-danger': '#a8402f',
      '--status-danger-soft': '#fdeeea',
      '--status-danger-border': '#f0cfc6',
      '--status-warning': '#8a6412',
      '--status-warning-soft': '#fdf6e4',
      '--status-warning-border': '#efdcaa',
      '--status-info': '#2a6ba1',
      '--status-info-soft': '#e9f1f9',
      '--status-info-border': '#c6dcef',
      '--status-neutral': '#6b6151',
      '--status-neutral-soft': '#f0ece4',
      '--status-neutral-border': '#ddd5c8',
      '--shadow-xs': '0 1px 2px rgba(60, 45, 20, 0.06)',
      '--shadow-sm': '0 1px 3px rgba(60, 45, 20, 0.09), 0 1px 2px rgba(60, 45, 20, 0.05)',
      '--shadow-lg': '0 18px 60px rgba(45, 33, 12, 0.24)',
      '--focus-ring-color': '#0f7b6c',
    },
  },

  // ---------- زمردي: داكن بارد بلكنة زمردية ----------
  emerald: {
    scheme: 'dark',
    tokens: {
      '--surface-page': '#08120f',
      '--surface-card': '#101b18',
      '--surface-elevated': '#14201c',
      '--surface-muted': '#172420',
      '--surface-inset': '#0c1714',
      '--surface-strong': '#1a2b25',
      '--surface-hover': '#14201d',
      '--surface-overlay': 'rgba(2, 10, 7, 0.72)',
      '--surface-navy': '#061109',
      '--surface-navy-raised': '#12211b',
      '--surface-navy-active': '#1c342b',
      '--surface-navy-border': '#1c2e28',
      '--text-on-dark': '#cfe6dc',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#8fa89d',
      '--border': '#233831',
      '--border-subtle': '#1c2e28',
      '--border-strong': '#325246',
      '--text-strong': '#e6f2ec',
      '--text-base': '#c2d6cd',
      '--text-muted': '#93a99f',
      '--text-subtle': '#7a8f85',
      '--text-inverse': '#08120f',
      '--accent': '#34d399',
      '--accent-strong': '#6ee7b7',
      '--accent-soft': '#14312a',
      '--accent-contrast': '#052e22',
      '--brand-navy': '#061109',
      '--brand-gold': '#d9b45f',
      '--brand-gold-deep': '#b8933f',
      '--status-success': '#34d399',
      '--status-success-soft': '#0f2c22',
      '--status-success-border': '#1f5a44',
      '--status-danger': '#f87171',
      '--status-danger-soft': '#301617',
      '--status-danger-border': '#5c2a2a',
      '--status-warning': '#fbbf24',
      '--status-warning-soft': '#2e2410',
      '--status-warning-border': '#5a4718',
      '--status-info': '#7dd3fc',
      '--status-info-soft': '#12293a',
      '--status-info-border': '#24506b',
      '--status-neutral': '#9fb3aa',
      '--status-neutral-soft': '#182a24',
      '--status-neutral-border': '#2a4038',
      '--shadow-xs': '0 1px 2px rgba(0, 0, 0, 0.45)',
      '--shadow-sm': '0 1px 3px rgba(0, 0, 0, 0.5), 0 1px 2px rgba(0, 0, 0, 0.32)',
      '--shadow-lg': '0 18px 60px rgba(0, 0, 0, 0.6)',
      '--focus-ring-color': '#6ee7b7',
    },
  },

  // ---------- تباين عالٍ: ثيم إتاحة، أسود صافٍ وحدود بيضاء ----------
  contrast: {
    scheme: 'dark',
    tokens: {
      '--surface-page': '#000000',
      '--surface-card': '#0a0a0a',
      '--surface-elevated': '#121212',
      '--surface-muted': '#171717',
      '--surface-inset': '#050505',
      '--surface-strong': '#1f1f1f',
      '--surface-hover': '#141414',
      '--surface-overlay': 'rgba(0, 0, 0, 0.85)',
      '--surface-navy': '#000000',
      '--surface-navy-raised': '#121212',
      '--surface-navy-active': '#1f1f1f',
      '--surface-navy-border': '#ffffff',
      '--text-on-dark': '#ffffff',
      '--text-on-dark-strong': '#ffffff',
      '--text-on-dark-muted': '#e0e0e0',
      '--border': '#ffffff',
      '--border-subtle': '#9a9a9a',
      '--border-strong': '#ffffff',
      '--text-strong': '#ffffff',
      '--text-base': '#f5f5f5',
      '--text-muted': '#d4d4d4',
      '--text-subtle': '#bdbdbd',
      '--text-inverse': '#000000',
      '--accent': '#ffe066',
      '--accent-strong': '#fff3a3',
      '--accent-soft': '#332b00',
      '--accent-contrast': '#000000',
      '--brand-navy': '#000000',
      '--brand-gold': '#ffd700',
      '--brand-gold-deep': '#ffc400',
      '--status-success': '#4ade80',
      '--status-success-soft': '#07230f',
      '--status-success-border': '#2f7a4a',
      '--status-danger': '#ff6b6b',
      '--status-danger-soft': '#2b0a0a',
      '--status-danger-border': '#a03030',
      '--status-warning': '#ffd93d',
      '--status-warning-soft': '#2b2200',
      '--status-warning-border': '#a08000',
      '--status-info': '#60d5ff',
      '--status-info-soft': '#062430',
      '--status-info-border': '#1f6f8f',
      '--status-neutral': '#d4d4d4',
      '--status-neutral-soft': '#1a1a1a',
      '--status-neutral-border': '#8a8a8a',
      '--shadow-xs': '0 1px 2px rgba(0, 0, 0, 0.9)',
      '--shadow-sm': '0 1px 3px rgba(0, 0, 0, 0.95)',
      '--shadow-lg': '0 18px 60px rgba(0, 0, 0, 0.95)',
      '--focus-ring-color': '#ffe066',
    },
  },
};

/** أسماء الثيمات بالترتيب المعروض. */
export const themeOrder = ['light', 'dark', 'midnight', 'sand', 'emerald', 'contrast'];

const canonical = Object.keys(themes.light.tokens);
for (const [id, theme] of Object.entries(themes)) {
  const keys = Object.keys(theme.tokens);
  const missing = canonical.filter((token) => !keys.includes(token));
  const extra = keys.filter((token) => !canonical.includes(token));
  if (missing.length || extra.length) {
    throw new Error(`الثيم ${id} غير مكتمل — ناقص: ${missing.join(',')} | زائد: ${extra.join(',')}`);
  }
}

const renderTokens = (tokens, indent = '  ') =>
  Object.entries(tokens).map(([name, value]) => `${indent}${name}: ${value};`).join('\n');

const header = `/* =============================================================================
   NEXUS — طبقة رموز التصميم (Design Tokens)
   ⚠️ ملف مولَّد آلياً — لا يُحرَّر يدوياً. المصدر: tools/theme/generate-tokens.mjs
   أعِد التوليد بـ: node tools/theme/generate-tokens.mjs

   الغاية: مصدر واحد للون والسطح والحد والطباعة والمسافة. كل ثيم يعلن **المجموعة
   الدلالية الكاملة نفسها**، فلا يرث ثيمٌ قيمةَ ثيمٍ آخر داخل معاينة أو تبديل.

   عيب مؤكَّد أُصلح (CQ-01): النسخة السابقة عرّفت 20 رمزاً بصيغة \`--border: var(--border)\`
   أي دورة CSS على الذات — قيمة غير صالحة وقت الحساب — فسقطت 89 استخداماً لـ\`var()\`
   بلا قيمة احتياطية: كل الحدود والأسطح والألوان الدلالية. هنا كل رمز يُعرَّف بقيمة
   حرفية مرة واحدة، ولا يشير أي رمز إلى نفسه.

   الثيمات: ${themeOrder.join(' · ')}
   مجموعة الرموز الدلالية لكل ثيم: ${canonical.length} رمزاً
   ============================================================================= */

:root {
  color-scheme: light;

  /* ===== الرموز البنيوية: لا تتغيّر بالثيم ===== */
${renderTokens(structural)}

  /* ===== اللوحة الدلالية الافتراضية (الفاتح) ===== */
${renderTokens(themes.light.tokens)}
}

/* =============================================================================
   اللوحات الدلالية لكل ثيم — كل كتلة تعلن المجموعة الكاملة نفسها.
   ============================================================================= */

/* فاتح باختيار صريح — ويشمل النطاق الفرعي حتى تعمل معاينة الثيم داخل الإعدادات. */
[data-theme='light'] {
  color-scheme: light;
${renderTokens(themes.light.tokens)}
}

/* النظام: يتبع سمة ويندوز فقط عندما لا يختار المستخدم ثيماً صريحاً. */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme]) {
    color-scheme: dark;
${renderTokens(themes.dark.tokens, '    ')}
  }
}

`;

const themeBlocks = themeOrder
  .filter((id) => id !== 'light')
  .map(
    (id) => `/* ${id} */\n[data-theme='${id}'] {\n  color-scheme: ${themes[id].scheme};\n${renderTokens(themes[id].tokens)}\n}\n`,
  )
  .join('\n');

const contrastExtras = `
/* =============================================================================
   إضافات ثيم التباين العالي: سماكة حدود أعلى وحلقة تركيز أوسع — معايير الإتاحة
   لا تتحقق بالألوان وحدها.
   ============================================================================= */
[data-theme='contrast'] {
  --focus-ring-width: 4px;
  --focus-ring-offset: 3px;
}

[data-theme='contrast'] .panel,
[data-theme='contrast'] .metric-card,
[data-theme='contrast'] .provider-row,
[data-theme='contrast'] .model-row,
[data-theme='contrast'] .theme-option,
[data-theme='contrast'] .attention-card,
[data-theme='contrast'] .attention-summary__card,
[data-theme='contrast'] .task-dialog,
[data-theme='contrast'] .settings-nav { border-width: 2px; }
`;

await writeFile(OUT, `${header}${themeBlocks}${contrastExtras}`, 'utf8');

// ترتيب الثيمات يُولَّد أيضاً حتى يكون مصدراً واحداً للواجهة والمولّد معاً،
// ويمنع انحراف قائمة الثيمات بين CSS والشيفرة.
const orderFile = `// ملف مولَّد آلياً — لا يُحرَّر يدوياً. المصدر: tools/theme/generate-tokens.mjs
// ترتيب الثيمات المعروض في الإعدادات، ويقابله كتلة \`[data-theme='…']\` في tokens.css.
export const themeOrder = [
${themeOrder.map((id) => `  '${id}',`).join('\n')}
] as const;
`;
await writeFile('D:/NEXUS/apps/desktop/src/lib/theme-order.ts', orderFile, 'utf8');

console.log(`WROTE ${OUT}`);
console.log('WROTE apps/desktop/src/lib/theme-order.ts');
console.log(`THEMES=${themeOrder.length} CANONICAL_TOKENS=${canonical.length}`);
