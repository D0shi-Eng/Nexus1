import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { complianceStatuses, dataModes, obligationPriorities, obligationStatuses, reviewStatuses, sourceTypes } from '@nexus/contracts';
import { ar } from '../../locales/ar';
import { en } from '../../locales/en';

type StatusDictionary = Readonly<Record<string, string>>;

const assertTranslated = (dictionary: StatusDictionary, values: readonly string[]): void => {
  for (const value of values) {
    expect(dictionary[value]).toBeTruthy();
  }
};

describe('obligation contract localization', () => {
  it('translates every typed obligation enum in Arabic and English', () => {
    const values = [...obligationStatuses, ...obligationPriorities, ...complianceStatuses, ...reviewStatuses, ...sourceTypes, ...dataModes];
    assertTranslated(ar.status, values);
    assertTranslated(en.status, values);
  });

  it('keeps Rust enum variants aligned with the shared serialized contract values', () => {
    const rustModels = readFileSync(resolve(process.cwd(), 'src-tauri/src/domain/models.rs'), 'utf8');
    const expectedVariants = [
      ['ObligationStatus', 'Open', 'InProgress', 'AtRisk', 'Completed', 'RequiresReview'],
      ['ObligationPriority', 'Critical', 'High', 'Medium', 'Low'],
      ['ComplianceStatus', 'Compliant', 'PartiallyCompliant', 'NonCompliant', 'NotAssessed', 'RequiresReview'],
      ['ReviewStatus', 'NotReviewed', 'InReview', 'Reviewed', 'Rejected'],
      ['SourceType', 'Official', 'Internal', 'Demo', 'Synthetic', 'UserProvided', 'Unknown'],
      ['DataMode', 'DemoSynthetic', 'Production'],
    ] as const;
    for (const [enumName, ...variants] of expectedVariants) {
      const declaration = new RegExp(`pub enum ${enumName} \\{([\\s\\S]*?)\\n\\}`);
      const body = rustModels.match(declaration)?.[1] ?? '';
      for (const variant of variants) {
        expect(body).toContain(`${variant},`);
      }
    }
  });

  it('keeps all dedicated obligation namespace keys in both locales', () => {
    expect(Object.keys(ar.obligations).sort()).toEqual(Object.keys(en.obligations).sort());
    expect(Object.keys(ar.obligations.summary).sort()).toEqual(Object.keys(en.obligations.summary).sort());
    expect(Object.keys(ar.obligations.filters).sort()).toEqual(Object.keys(en.obligations.filters).sort());
    expect(Object.keys(ar.obligations.details).sort()).toEqual(Object.keys(en.obligations.details).sort());
    expect(Object.keys(ar.obligations.edit).sort()).toEqual(Object.keys(en.obligations.edit).sort());
  });
});
