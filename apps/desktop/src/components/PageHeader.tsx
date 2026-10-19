import type { ComponentType, ReactNode } from 'react';

// ترويسة الصفحة الموحّدة: سطر تمهيدي + عنوان + وصف + إجراءات أساسية.
// كل صفحة تمر من هنا، فتبقى المسافات وتسلسل العناوين متطابقة بين الوحدات الـ15.
type PageHeaderProps = {
  readonly eyebrow: string;
  readonly eyebrowIcon: ComponentType<{ size?: number; 'aria-hidden'?: boolean | 'true' | 'false' }>;
  readonly title: string;
  readonly titleId: string;
  readonly description?: string;
  readonly actions?: ReactNode;
};

export const PageHeader = ({ eyebrow, eyebrowIcon: Icon, title, titleId, description, actions }: PageHeaderProps) => (
  <header className="page-heading">
    <div>
      <span className="eyebrow"><Icon size={14} aria-hidden="true" /> {eyebrow}</span>
      <h1 id={titleId}>{title}</h1>
      {description ? <p>{description}</p> : null}
    </div>
    {actions ? <div className="page-heading__actions">{actions}</div> : null}
  </header>
);
