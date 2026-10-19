import { useQuery } from '@tanstack/react-query';
import type { ReactNode } from 'react';

import { nexusApi } from './nexus-api';
import { SessionContext, type SessionState } from './use-session';

// مزوّد سياق الجلسة: يجلب فاعل الديمو النشط وصلاحياته الفعلية من الخلف حصراً.
// القيم هنا عرضية فقط — كل باب تفويض حقيقي يبقى في النواة.
export const SessionProvider = ({ children }: { children: ReactNode }) => {
  const sessionQuery = useQuery({
    queryKey: ['session-context'],
    queryFn: nexusApi.getSessionContext,
    staleTime: 30_000,
  });

  const value: SessionState = sessionQuery.data
    ? {
      actorId: sessionQuery.data.actorId,
      displayName: sessionQuery.data.displayName,
      responsibility: sessionQuery.data.responsibility,
      permissions: sessionQuery.data.permissions,
      can: (permission: string) => sessionQuery.data?.permissions.includes(permission) ?? true,
    }
    : {
      actorId: '',
      displayName: '',
      responsibility: '',
      permissions: [],
      // قبل اكتمال التحميل تبقى الأزرار ظاهرة؛ الرفض الخلفي هو الضمانة الفعلية.
      can: () => true,
    };

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
};
