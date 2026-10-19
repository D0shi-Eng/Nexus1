import { createContext, useContext } from 'react';

// سياق الصلاحيات للعرض فقط: الواجهة تخفي ما لا يملكه الفاعل تحسيناً للتجربة،
// والتفويض الحقيقي يبقى خلفياً مصدراً واحداً — هذا السياق لا يمنح أي سلطة.
export interface SessionState {
  readonly actorId: string;
  readonly displayName: string;
  readonly responsibility: string;
  readonly permissions: readonly string[];
  readonly can: (permission: string) => boolean;
}

export const SessionContext = createContext<SessionState>({
  actorId: '',
  displayName: '',
  responsibility: '',
  permissions: [],
  // أثناء عدم التحميل نعرض التحكم — منع الإخفاء الخاطئ قبل معرفة الصلاحيات؛
  // الخلف يظل الحكم النهائي فيرفض ما لا يملكه الفاعل بصدق.
  can: () => true,
});

export const useSession = (): SessionState => useContext(SessionContext);
