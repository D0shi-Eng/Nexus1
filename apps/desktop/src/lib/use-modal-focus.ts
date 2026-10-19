import { useCallback, useEffect, useRef } from 'react';

// اختيار العناصر القابلة للتركيز داخل الحوار — نفس معيار النمط المُثبت في AgentDialog.
const FOCUSABLE_SELECTOR = 'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface ModalFocusController {
  /** مرجع عنصر الحوار نفسه (section role="dialog"). */
  readonly dialogRef: React.RefObject<HTMLElement | null>;
  /** مرجع استدعائي للعنصر الذي يستقبل التركيز أول افتتاح الحوار (زر الإغلاق عادةً). */
  readonly initialFocusRef: (element: HTMLElement | null) => void;
}

/**
 * إدارة تركيز الحواريات المشتركة: نقل التركيز عند الفتح، حصر Tab وShift+Tab داخلياً،
 * إغلاق بمفتاح Escape، وإعادة التركيز إلى عنصر المشغّل عند الإغلاق.
 *
 * استخدام واحد لكل حوار؛ الاعتماد على مرجع onClose يمنع إعادة تركيب المؤثرات
 * عند إعادة رسم المكوّن الأب.
 */
export const useModalFocus = (onClose: () => void): ModalFocusController => {
  const dialogRef = useRef<HTMLElement | null>(null);
  const initialFocusTarget = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  // مرجع استدعائي ثابت يقبل أي عنصر — يلائم أزرار الإغلاق وحقول البحث معاً.
  const initialFocusRef = useCallback((element: HTMLElement | null): void => {
    initialFocusTarget.current = element;
  }, []);

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    (initialFocusTarget.current ?? dialogRef.current?.querySelector<HTMLElement>(FOCUSABLE_SELECTOR) ?? dialogRef.current)?.focus();

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== 'Tab') {
        return;
      }
      // الحوار يملك دورة Tab بالكامل: كل ضغطة تُلغى ويُنقل التركيز صراحةً داخل الحلقة،
      // فلا يعتمد الاحتواء على سلوك المتصفح الافتراضي ولا يوجد مسار هروب للخلفية.
      const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (!focusable || focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const index = Array.prototype.indexOf.call(focusable, document.activeElement);
      event.preventDefault();
      if (event.shiftKey) {
        const previous = index <= 0 ? focusable[focusable.length - 1] : focusable[index - 1];
        previous.focus();
      } else {
        const next = index === -1 || index === focusable.length - 1 ? focusable[0] : focusable[index + 1];
        next.focus();
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      previousFocus?.focus();
    };
  }, []);

  return { dialogRef, initialFocusRef };
};
