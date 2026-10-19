import { useState } from 'react';
import { Archive, ArchiveRestore, CircleAlert, DatabaseBackup, LoaderCircle, RefreshCw, TriangleAlert } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import type { BackupManifest } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { formatDate } from '../../lib/format';

// عرض حجم النسخة بوحدة ثنائية مقروءة — القيم المخزنة بايتات خام.
const formatBytes = (bytes: number): string =>
  bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;

// النسخ الاحتياطي والاستعادة: مجلد الوجهة يبقى في ذاكرة الواجهة فقط ولا يُخزن أبداً.
// الاستعادة إجراء مدمر — التأكيد صريح بنص تحذيري ونسخة أمان تلقائية تُنشأ خلفياً قبلها.
export const BackupPanel = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [targetDirectory, setTargetDirectory] = useState<string | null>(null);
  const [pendingRestoreFile, setPendingRestoreFile] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mutationError, setMutationError] = useState<string | null>(null);

  const backupsQuery = useQuery({
    queryKey: ['backups', targetDirectory],
    enabled: targetDirectory !== null,
    queryFn: () => nexusApi.listBackups({ targetDirectory: targetDirectory ?? '' }),
    staleTime: 30_000,
  });

  const createBackupMutation = useMutation({ mutationFn: nexusApi.createBackup });
  const restoreBackupMutation = useMutation({ mutationFn: nexusApi.restoreBackup });

  const createBackup = async (): Promise<void> => {
    setFeedback(null);
    setMutationError(null);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== 'string') {
        return;
      }
      const result = await createBackupMutation.mutateAsync({ targetDirectory: selected });
      setTargetDirectory(selected);
      setFeedback(t('feedback.backupCreated', { fileName: result.fileName, sha: result.sha256.slice(0, 12) }));
      await queryClient.invalidateQueries({ queryKey: ['backups'] });
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const browseRestoreFile = async (): Promise<void> => {
    setMutationError(null);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'NEXUS Backup', extensions: ['nxbackup'] }],
      });
      if (typeof selected === 'string') {
        setPendingRestoreFile(selected);
      }
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const restoreBackup = async (): Promise<void> => {
    if (!pendingRestoreFile) {
      return;
    }
    setFeedback(null);
    setMutationError(null);
    try {
      const result = await restoreBackupMutation.mutateAsync({ backupFile: pendingRestoreFile, confirm: true });
      setPendingRestoreFile(null);
      setFeedback(t('feedback.backupRestored', {
        mode: t(`status.${result.restoredMode}`, { defaultValue: result.restoredMode }),
        safetyCopyFile: result.safetyCopyFile,
      }));
      // الاستعادة تستبدل مساحة العمل كلها — إبطال كل الاستعلامات، والبوابة تُعيد الدخول إن انتهت الجلسة.
      await queryClient.invalidateQueries();
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const backups: readonly BackupManifest[] = backupsQuery.data ?? [];
  const listErrorCode = backupsQuery.error instanceof NexusApiError ? backupsQuery.error.code : 'LOCAL_SERVICE_UNAVAILABLE';

  return (
    <section className="panel" aria-labelledby="settings-backup-title">
      <div className="panel__header">
        <div><h2 id="settings-backup-title">{t('backup.title')}</h2></div>
        <DatabaseBackup size={18} aria-hidden="true" />
      </div>

      <div className="backup-actions">
        <button className="button button--secondary" type="button" onClick={() => { void createBackup(); }} disabled={createBackupMutation.isPending}>
          {createBackupMutation.isPending
            ? <LoaderCircle className="spin" size={16} aria-hidden="true" />
            : <Archive size={16} aria-hidden="true" />}
          {t('backup.create')}
        </button>
        <button className="button button--secondary" type="button" onClick={() => { void browseRestoreFile(); }} disabled={restoreBackupMutation.isPending}>
          <ArchiveRestore size={16} aria-hidden="true" />
          {t('backup.restore')}
        </button>
      </div>
      <p className="form-note">{t('backup.subtitle')}</p>

      {feedback ? <p className="form-note" role="status">{feedback}</p> : null}
      {mutationError ? (
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t(`errors.${mutationError}`, { defaultValue: t('errors.unavailable') })}</p>
        </div>
      ) : null}

      {pendingRestoreFile ? (
        <div className="backup-restore-warning" role="alert">
          <TriangleAlert size={20} aria-hidden="true" />
          <div>
            <h3>{t('backup.restoreTitle')}</h3>
            <p>{t('backup.restoreWarning')}</p>
            <p><strong>{t('backup.restoreFile')}:</strong> <code dir="ltr">{pendingRestoreFile}</code></p>
            <div className="backup-restore-warning__actions">
              <button className="button button--danger" type="button" onClick={() => { void restoreBackup(); }} disabled={restoreBackupMutation.isPending}>
                {restoreBackupMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : null}
                {t('backup.restoreConfirm')}
              </button>
              <button className="button button--secondary" type="button" onClick={() => setPendingRestoreFile(null)} disabled={restoreBackupMutation.isPending}>
                {t('backup.restoreCancel')}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      <div className="backup-list">
        <div className="backup-list__header">
          <h3>{t('backup.listTitle')}</h3>
          <button
            className="icon-button"
            type="button"
            onClick={() => { void backupsQuery.refetch(); }}
            aria-label={t('backup.refresh')}
            title={t('backup.refresh')}
            disabled={targetDirectory === null || backupsQuery.isFetching}
          >
            <RefreshCw size={16} className={backupsQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
          </button>
        </div>
        {targetDirectory === null ? (
          <p className="form-note">{t('backup.noDirectory')}</p>
        ) : backupsQuery.isError ? (
          <div className="error-panel" role="alert">
            <CircleAlert size={20} aria-hidden="true" />
            <p>{t(`errors.${listErrorCode}`, { defaultValue: t('errors.unavailable') })}</p>
          </div>
        ) : backups.length > 0 ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>{t('backup.table.created')}</th>
                  <th>{t('backup.table.mode')}</th>
                  <th>{t('backup.table.schema')}</th>
                  <th>{t('backup.table.size')}</th>
                  <th>{t('backup.table.fingerprint')}</th>
                </tr>
              </thead>
              <tbody>
                {backups.map((backup) => (
                  <tr key={`${backup.createdAt}-${backup.sha256}`}>
                    <td><time dateTime={backup.createdAt}>{formatDate(backup.createdAt, language)}</time></td>
                    <td>{t(`status.${backup.mode}`, { defaultValue: backup.mode })}</td>
                    <td>{backup.schemaVersion}</td>
                    <td>{formatBytes(backup.bytes)}</td>
                    <td><code dir="ltr">{backup.sha256.slice(0, 12)}</code></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="form-note">{t('backup.listEmpty')}</p>
        )}
      </div>
    </section>
  );
};
