import { describe, expect, it } from 'vitest';
import {
  agentRunStatuses, agentStatuses, applicabilityStates, attentionActions, attentionItemKinds,
  attentionItemStatuses, attentionReasonCodes, complianceStatuses, dataClassifications, dataModes,
  delegationStatuses, demoActorResponsibilities, evidenceStatuses, integrationProviderStates,
  knowledgeSourceStatuses, moduleAvailabilityStatuses, obligationPriorities, obligationStatuses,
  organizationalIntelligencePriorityKinds, organizationalIntelligencePriorityReasons,
  organizationalIntelligencePriorityStates, planStatuses, planStepStatuses, regulatorySourceProvenances,
  reviewStatuses, riskStatuses, severityLevels, skillStatuses, sourceTypes, taskPriorities, taskStatuses,
  toolExternalities, toolInvocationStatuses, toolMutations, toolPolicyDecisions, toolStatuses,
  verificationStates, workspaceUserStatuses,
} from '@nexus/contracts';
import { ar } from './ar';
import { en } from './en';

type TranslationNode = string | { readonly [key: string]: TranslationNode };

const flatten = (node: TranslationNode, prefix = ''): [string, string][] =>
  Object.entries(node).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof value === 'string' ? [[path, value] as [string, string]] : flatten(value, path);
  });

const arabicEntries = flatten(ar as TranslationNode);
const englishList = flatten(en as TranslationNode);
const englishEntries = new Map(englishList);

// قيم مستثناة عن قصد لأنها أسماء أعلام لا ترجمات: اسم المنتج، والأسماء الذاتية للغتين
// (يجب أن تظهر كلغة نفسها داخل الواجهتين)، ورمز عمود الرقم، ومثال عنوان واجهة برمجية
// (عنوان تقني لا يُترجم، ويبقى بالاتينية في اللغتين ليُنسخ كما هو).
const INTENTIONALLY_LANGUAGE_NEUTRAL = new Map<string, string>([
  ['product.name', 'NEXUS'],
  ['settings.language.ar', 'العربية'],
  ['settings.language.en', 'English'],
  ['plans.stepTable.index', '#'],
  ['ai.providers.fields.baseUrlPlaceholder', 'https://api.example.com/v1'],
]);

const ARABIC_SCRIPT = /[\u0621-\u064A]/;
const LATIN_SCRIPT = /[A-Za-z]/;
const RAW_TECHNICAL_CODE = /^[A-Z][A-Z0-9_]{2,}$/;
const PLACEHOLDER = /\{\{[^}]+\}\}/g;

const readPath = (node: TranslationNode, path: string): string | undefined =>
  path.split('.').reduce<TranslationNode | undefined>(
    (current, segment) => (current && typeof current === 'object' ? current[segment] : undefined),
    node,
  ) as string | undefined;

// خرائط الترجمة الفعلية التي تُبنى مفاتيحها ديناميكياً في المكوّنات.
// كل تعداد مستورد من العقود يُفحص هنا، فلا يمكن أن يُضاف رمز خلفي جديد
// ويظهر للمستخدم خاماً أو كمفتاح منقّط بلا كشف الاختبار.
const dynamicNamespaces: readonly { readonly namespace: string; readonly values: readonly string[] }[] = [
  {
    namespace: 'status',
    values: [
      ...obligationStatuses, ...obligationPriorities, ...complianceStatuses, ...reviewStatuses,
      ...sourceTypes, ...dataModes, ...severityLevels, ...riskStatuses, ...taskStatuses, ...taskPriorities,
      ...verificationStates, ...evidenceStatuses, ...applicabilityStates, ...attentionItemStatuses,
      ...organizationalIntelligencePriorityStates, ...dataClassifications, ...agentStatuses, ...skillStatuses,
      ...toolStatuses, ...workspaceUserStatuses,
    ],
  },
  { namespace: 'moduleAvailability.status', values: moduleAvailabilityStatuses },
  { namespace: 'attention.kind', values: attentionItemKinds },
  { namespace: 'attention.reason', values: attentionReasonCodes },
  { namespace: 'attention.action', values: attentionActions },
  { namespace: 'agentRuns.status', values: agentRunStatuses },
  { namespace: 'agentRuns.outcome', values: ['COMPLETED', 'FAILED'] },
  { namespace: 'agentRuns.invocations.status', values: toolInvocationStatuses },
  { namespace: 'agentRuns.invocations.decision', values: toolPolicyDecisions },
  { namespace: 'integrations.state', values: integrationProviderStates },
  { namespace: 'integrations.providerKind', values: ['LOCAL'] },
  { namespace: 'plans.statusLabel', values: [...planStatuses, ...planStepStatuses, ...delegationStatuses] },
  {
    namespace: 'plans.verificationOutcome',
    values: ['SUPPORTED', 'PARTIALLY_SUPPORTED', 'UNSUPPORTED', 'CONFLICTING', 'UNKNOWN', 'INSUFFICIENT_EVIDENCE', 'NOT_VERIFIED'],
  },
  { namespace: 'organizationalIntelligence.priorityKind', values: organizationalIntelligencePriorityKinds },
  { namespace: 'organizationalIntelligence.priorityReason', values: organizationalIntelligencePriorityReasons },
  { namespace: 'regulatory.provenance', values: regulatorySourceProvenances },
  { namespace: 'identity.responsibility', values: [...demoActorResponsibilities, 'ADMINISTRATOR', 'MEMBER'] },
  { namespace: 'agentsTools.effect', values: [...toolMutations, ...toolExternalities] },
  { namespace: 'knowledge.status', values: knowledgeSourceStatuses },
  // منشأ المعرفة غير مُصدَّر كتعداد من العقود — القيم مأخوذة من تعداد الخلف.
  {
    namespace: 'knowledge.provenanceLabel',
    values: ['USER_PROVIDED', 'NEXUS_INTERNAL', 'DEMO_SYNTHETIC', 'EXTERNAL_REFERENCE', 'UNKNOWN'],
  },
  {
    namespace: 'truthClass',
    values: ['FACT', 'VERIFIED', 'CLAIM', 'ASSUMPTION', 'INFERENCE', 'OPINION', 'UNKNOWN', 'CONFLICTING', 'AI_INTERPRETATION'],
  },
  { namespace: 'memory.type', values: ['WORKING', 'EPISODIC', 'SEMANTIC', 'ORGANIZATIONAL', 'AGENT'] },
  { namespace: 'memory.status', values: ['ACTIVE', 'PROPOSED', 'DISABLED'] },
  { namespace: 'memory.scope', values: ['AGENT_RUN', 'PLAN', 'TENANT', 'AGENT', 'OBLIGATION'] },
  {
    namespace: 'users.role',
    values: ['TENANT_ADMIN', 'COMPLIANCE_READER', 'COMPLIANCE_OFFICER', 'READER', 'OBLIGATION_MANAGER', 'ADMINISTRATOR', 'REVIEWER', 'REQUESTER', 'MEMBER'],
  },
];

describe('locale correctness beyond key parity', () => {
  it('never leaves a message in the wrong language', () => {
    const leakedIntoEnglish = englishList
      .filter(([key, value]) => !INTENTIONALLY_LANGUAGE_NEUTRAL.has(key) && ARABIC_SCRIPT.test(value))
      .map(([key]) => key);
    expect(leakedIntoEnglish).toEqual([]);

    // النص العربي في ar يجب أن يحمل حروفاً عربية فعلاً، وإلا فهو نص لاتيني تسرّب.
    const leakedIntoArabic = arabicEntries
      .filter(([key, value]) => !INTENTIONALLY_LANGUAGE_NEUTRAL.has(key) && !ARABIC_SCRIPT.test(value) && LATIN_SCRIPT.test(value))
      .map(([key]) => key);
    expect(leakedIntoArabic).toEqual([]);
  });

  it('keeps interpolation placeholders identical in both languages', () => {
    const mismatched = arabicEntries
      .filter(([key, value]) => {
        const english = englishEntries.get(key);
        if (english === undefined) {
          return false;
        }
        const arPlaceholders = (value.match(PLACEHOLDER) ?? []).sort();
        const enPlaceholders = (english.match(PLACEHOLDER) ?? []).sort();
        return arPlaceholders.join(',') !== enPlaceholders.join(',');
      })
      .map(([key]) => key);
    expect(mismatched).toEqual([]);
  });

  it('stores user-facing text, never a raw technical code', () => {
    const rawCodes = arabicEntries
      .filter(([key, value]) => !INTENTIONALLY_LANGUAGE_NEUTRAL.has(key) && RAW_TECHNICAL_CODE.test(value.trim()))
      .map(([key, value]) => `${key}=${value}`);
    expect(rawCodes).toEqual([]);
  });

  it('never leaves a value equal to its own key (untranslated key rendered raw)', () => {
    const untranslated = arabicEntries.filter(([key, value]) => value.trim() === key).map(([key]) => key);
    expect(untranslated).toEqual([]);
  });

  it('resolves every dynamically built translation key in both languages', () => {
    const missing: string[] = [];
    for (const { namespace, values } of dynamicNamespaces) {
      for (const value of [...new Set(values)]) {
        const path = `${namespace}.${value}`;
        if (typeof readPath(ar as TranslationNode, path) !== 'string') {
          missing.push(`ar:${path}`);
        }
        if (typeof readPath(en as TranslationNode, path) !== 'string') {
          missing.push(`en:${path}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });
});
