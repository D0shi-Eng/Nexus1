import { describe, expect, it } from 'vitest';
import { attentionActions, attentionItemKinds, attentionItemStatuses, attentionReasonCodes } from '@nexus/contracts';
import { ar } from '../../locales/ar';
import { en } from '../../locales/en';

describe('attention localization contract', () => {
  it('localizes every supported attention kind, status, reason, and action in Arabic and English', () => {
    for (const kind of attentionItemKinds) {
      expect(ar.attention.kind[kind]).toBeTruthy();
      expect(en.attention.kind[kind]).toBeTruthy();
    }

    for (const status of attentionItemStatuses) {
      expect(ar.status[status]).toBeTruthy();
      expect(en.status[status]).toBeTruthy();
    }

    for (const reason of attentionReasonCodes) {
      expect(ar.attention.reason[reason]).toBeTruthy();
      expect(en.attention.reason[reason]).toBeTruthy();
    }

    for (const action of attentionActions) {
      expect(ar.attention.action[action]).toBeTruthy();
      expect(en.attention.action[action]).toBeTruthy();
    }
  });
});
