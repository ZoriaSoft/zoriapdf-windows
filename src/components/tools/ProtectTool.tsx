import React, { useState, useEffect } from 'react';
import { FolderOpen, Eye, EyeOff, ShieldAlert } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const ProtectTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [newPassword, setNewPassword] = useState<string>('');
  const [existingPassword, setExistingPassword] = useState<string>(
    activeTab?.sourcePassword || ''
  );
  const [showPassword, setShowPassword] = useState<boolean>(false);
  const [outputPath, setOutputPath] = useState<string>('');

  useEffect(() => {
    setNewPassword('');
    setExistingPassword(activeTab?.sourcePassword || '');
    setShowPassword(false);
    setOutputPath('');
  }, [activeTab?.info.id, activeTab?.sourcePassword]);

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('sifrelenmis_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !newPassword || !outputPath.trim()) return;
    await executeOperation('protect', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      newPassword,
      password: existingPassword || undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Belgeyi güçlü parola ile şifreler. Şifresi bilinmeyen kişilerce açılamaz.
      </div>

      <div className="form-group">
        <label className="form-label">Yeni Şifre</label>
        <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
          <input
            type={showPassword ? 'text' : 'password'}
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            placeholder="Güçlü bir şifre belirleyin..."
            className="form-input"
            style={{ paddingRight: '36px' }}
          />
          <button
            type="button"
            onClick={() => setShowPassword(!showPassword)}
            style={{
              position: 'absolute',
              right: '8px',
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--color-text-subtle)',
              display: 'flex',
              alignItems: 'center',
            }}
          >
            {showPassword ? <EyeOff size={15} /> : <Eye size={15} />}
          </button>
        </div>
      </div>

      {activeTab.sourcePassword && (
        <div className="form-group">
          <label className="form-label">Mevcut Şifre (Algılandı)</label>
          <input
            type="password"
            value={existingPassword}
            onChange={(e) => setExistingPassword(e.target.value)}
            className="form-input"
          />
        </div>
      )}

      <div className="form-group">
        <label className="form-label">Çıktı Dosyası (.pdf)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            placeholder="Kaydedilecek dosya yolu..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickOutput}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Dosya Seç"
          >
            <FolderOpen size={14} />
          </button>
        </div>
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: '6px',
          padding: '8px',
          backgroundColor: 'var(--color-warning-subtle)',
          borderRadius: 'var(--radius-xs)',
          fontSize: '11px',
          color: 'var(--color-warning)',
        }}
      >
        <ShieldAlert size={14} style={{ flexShrink: 0, marginTop: '1px' }} />
        <span>Belirlediğiniz şifreyi lütfen unutmayın. Kaybedilen şifreler geri getirilemez.</span>
      </div>

      <button
        type="button"
        className="btn btn-primary"
        onClick={handleRun}
        disabled={!newPassword || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Şifreleniyor...' : 'Belgeyi Şifrele ve Kaydet'}
      </button>
    </div>
  );
};
