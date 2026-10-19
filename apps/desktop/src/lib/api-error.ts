import { NexusApiError } from './nexus-api';

// استخراج رمز الخطأ الآمن القادم من الخلف.
// لا يُعاد أي نص داخلي أو تفصيل SQL؛ الواجهة تتعامل مع رموز معرَّفة فقط.
export const resolveErrorCode = (error: unknown): string =>
  error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE';
