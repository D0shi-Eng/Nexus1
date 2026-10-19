// عناصر التنقل الـ15 — المصدر الواحد لمفاتيح الوحدات في القشرة وحوار البحث.
// لكل وحدة أيقونة دلالية واحدة لا تتكرر: تكرارها يوهم بأن الوجهات المختلفة وحدة واحدة،
// فيفقد المستخدم التمييز البصري السريع داخل القائمة — والتمييز هو وظيفة الأيقونة الوحيدة.
import {
  AlertTriangle, BookOpen, Building2, ChartColumn, CheckCircle2, ClipboardCheck, FileCheck2,
  Gavel, LayoutDashboard, LockKeyhole, Plug, ScrollText, ShieldAlert, SlidersHorizontal, Sparkles,
} from 'lucide-react';

export const navigationItems = [
  { key: 'overview', icon: LayoutDashboard },
  { key: 'attention', icon: AlertTriangle },
  { key: 'obligations', icon: ClipboardCheck },
  { key: 'regulatory', icon: Gavel },
  { key: 'risks', icon: ShieldAlert },
  { key: 'evidence', icon: FileCheck2 },
  { key: 'tasks', icon: CheckCircle2 },
  { key: 'controls', icon: LockKeyhole },
  { key: 'agents', icon: Sparkles },
  { key: 'knowledge', icon: BookOpen },
  { key: 'reports', icon: ChartColumn },
  { key: 'audit', icon: ScrollText },
  { key: 'organization', icon: Building2 },
  { key: 'integrations', icon: Plug },
  { key: 'settings', icon: SlidersHorizontal },
] as const;

export type NavigationKey = (typeof navigationItems)[number]['key'];
