import React, { useState, useEffect } from 'react';
import { FolderOpen, Eye, EyeOff } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const UnlockTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [password, setPassword] = useState<string>(activeTab?.sourcePassword || '');
  const [showPassword, setShowPassword] = useState<boolean>(false);
  const [outputPath, setOutputPath] = useState<string>('');

  useEffect(() => {
    setPassword(activeTab?.sourcePassword || '');
    setShowPassword(false);
    setOutputPath('');
  }, [activeTab?.info.id, activeTab?.sourcePassword]);

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('sifresi_kaldirilmis_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !password || !outputPath.trim()) return;
    await executeOperation('unlock', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      password,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Belgedeki mevcut parola korumasını kaldırarak parolasız yeni bir PDF kopyası kaydeder.
      </div>

      <div className="form-group">
        <label className="form-label">Mevcut Belge Parolası</label>
        <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
          <input
            type={showPassword ? 'text' : 'password'}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Belge parolasını girin..."
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
        {activeTab.sourcePassword && (
          <span className="form-help" style={{ color: 'var(--color-accent)' }}>
            ✓ Bu sekmede açılırken kullanılan parola otomatik aktarıldı.
          </span>
        )}
      </div>

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

      <button
        type="button"
        className="btn btn-primary"
        onClick={handleRun}
        disabled={!password || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Şifre Kaldırılıyor...' : 'Şifreyi Kaldır ve Kaydet'}
      </button>
    </div>
  );
};
