import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useModalFocus } from './use-modal-focus';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// مكوّن اختبار يستهلك الـ primitive بنفس بنية الحواريات الحقيقية في التطبيق.
const ModalFocusProbe = ({ onClose }: { readonly onClose: () => void }) => {
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  return (
    <div>
      <button type="button" id="outside-trigger">مشغّل خارجي</button>
      <section ref={dialogRef} role="dialog" aria-modal="true" tabIndex={-1}>
        <button ref={initialFocusRef} type="button" id="dialog-close">إغلاق</button>
        <input id="dialog-input" />
        <button type="button" id="dialog-confirm">تأكيد</button>
      </section>
    </div>
  );
};
describe('useModalFocus', () => {
  let container: HTMLElement;
  let root: Root | null = null;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
  });

  afterEach(() => {
    act(() => {
      root?.unmount();
    });
    container.remove();
  });

  const mountProbe = (onClose: () => void = () => {}): void => {
    root = createRoot(container);
    act(() => {
      root!.render(<ModalFocusProbe onClose={onClose} />);
    });
  };

  const pressTab = (shiftKey: boolean): void => {
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey, bubbles: true }));
    });
  };

  it('moves focus into the dialog on open and restores the trigger focus on close', () => {
    const trigger = document.createElement('button');
    trigger.type = 'button';
    document.body.appendChild(trigger);
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    mountProbe();
    expect(document.activeElement?.id).toBe('dialog-close');

    act(() => {
      root?.unmount();
    });
    root = null;
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });

  it('keeps Tab cycling inside the dialog', () => {
    mountProbe();
    expect(document.activeElement?.id).toBe('dialog-close');

    pressTab(false);
    expect(document.activeElement?.id).toBe('dialog-input');
    pressTab(false);
    expect(document.activeElement?.id).toBe('dialog-confirm');
    // الالتفاف من الأخير إلى الأول — لا هروب إلى خلفية الصفحة.
    pressTab(false);
    expect(document.activeElement?.id).toBe('dialog-close');
  });

  it('keeps Shift+Tab cycling inside the dialog', () => {
    mountProbe();

    // الالتفاف من الأول إلى الأخير عكسياً.
    pressTab(true);
    expect(document.activeElement?.id).toBe('dialog-confirm');
    pressTab(true);
    expect(document.activeElement?.id).toBe('dialog-input');
    pressTab(true);
    expect(document.activeElement?.id).toBe('dialog-close');
  });

  it('returns focus into the dialog when it escapes the modal boundary', () => {
    mountProbe();
    const outside = document.getElementById('outside-trigger');
    outside?.focus();
    expect(document.activeElement).toBe(outside);

    pressTab(false);
    expect(document.activeElement?.id).toBe('dialog-close');
    pressTab(true);
    expect(document.activeElement?.id).toBe('dialog-confirm');
  });

  it('closes the dialog on Escape', () => {
    const onClose = vi.fn();
    mountProbe(onClose);

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
