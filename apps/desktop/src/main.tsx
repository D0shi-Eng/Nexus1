import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/global.css';
import { bootstrapTheme } from './lib/theme';
import './lib/i18n';
import { AuthGate } from './app/AuthGate';
import { SessionProvider } from './lib/session-context';

// تطبيق الثيم المخزّن قبل أول رسم — يمنع وميض الثيم الخاطئ عند الإقلاع.
// وعندما يكون التفضيل «النظام» لا تُكتب سمة، فيتولّى CSS اختيار اللوحة بلا JavaScript.
bootstrapTheme();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <SessionProvider>
        <AuthGate />
      </SessionProvider>
    </QueryClientProvider>
  </StrictMode>,
);
