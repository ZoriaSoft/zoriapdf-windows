import React, { useLayoutEffect, useRef, useState } from 'react';
import { Lock, Eye, EyeOff, X } from 'lucide-react';
import { useDocument } from '../context/DocumentContext';

export const PasswordModal: React.FC = () => {
  const { passwordPrompt, submitPasswordPrompt, cancelPasswordPrompt } = useDocument();
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const submitSequence = useRef(0);

  useLayoutEffect(() => {
    ++submitSequence.current;
    setPassword('');
    setShowPassword(false);
    setIsSubmitting(false);
    if (!passwordPrompt.isOpen) return;
    const previous = document.activeElement;
    inputRef.current?.focus();
    return () => {
      ++submitSequence.current;
      const dialog = document.querySelector<HTMLElement>('[aria-modal="true"]');
      if (dialog) {
        dialog.querySelector<HTMLElement>('input, button:not(:disabled), [tabindex="0"]')?.focus();
      } else if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus();
      }
    };
  }, [passwordPrompt.isOpen, passwordPrompt.path]);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      if (!isSubmitting) cancelPasswordPrompt();
    } else if (event.key === 'Tab') {
      const controls = modalRef.current?.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), [tabindex="0"]'
      );
      if (!controls?.length) return;
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  };

  if (!passwordPrompt.isOpen) return null;

  const fileName = passwordPrompt.path.split(/[/\\]/).pop() || passwordPrompt.path;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password) return;
    const sequence = ++submitSequence.current;
    setIsSubmitting(true);
    try {
      await submitPasswordPrompt(password);
    } finally {
      if (sequence === submitSequence.current) setIsSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={() => { if (!isSubmitting) cancelPasswordPrompt(); }}>
      <div
        ref={modalRef}
        onKeyDown={handleKeyDown}
        role="dialog"
        aria-modal="true"
        aria-label="Şifreli Belge"
        className="modal-content"
        style={{ maxWidth: '420px' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          style={{
            padding: '14px 18px',
            borderBottom: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontWeight: 600 }}>
            <div
              style={{
                width: '28px',
                height: '28px',
                borderRadius: '50%',
                backgroundColor: 'var(--color-accent-subtle)',
                color: 'var(--color-accent)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Lock size={15} />
            </div>
            <span>Şifreli Belge</span>
          </div>
          <button
            type="button"
            className="btn btn-ghost"
            style={{ padding: '4px' }}
            onClick={cancelPasswordPrompt}
            disabled={isSubmitting}
            aria-label="Şifre penceresini kapat"
          >
            <X size={15} />
          </button>
        </div>

        <form onSubmit={handleSubmit} style={{ padding: '18px' }}>
          <p
            style={{
              fontSize: '13px',
              color: 'var(--color-text-muted)',
              marginBottom: '14px',
              lineHeight: 1.4,
            }}
          >
            <strong style={{ color: 'var(--color-text)' }}>{fileName}</strong> belgesi şifrelenmiştir. Belgeyi açmak için lütfen şifresini girin.
          </p>

          <div className="form-group">
            <label className="form-label">Belge Şifresi</label>
            <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
              <input
                type={showPassword ? 'text' : 'password'}
                ref={inputRef}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Şifreyi yazın..."
                className="form-input"
                style={{ paddingRight: '36px' }}
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                aria-label={showPassword ? 'Şifreyi gizle' : 'Şifreyi göster'}
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

          {passwordPrompt.error && (
            <div
              style={{
                fontSize: '12px',
                color: 'var(--color-danger)',
                backgroundColor: 'var(--color-danger-subtle)',
                padding: '6px 10px',
                borderRadius: 'var(--radius-xs)',
                marginBottom: '14px',
              }}
            >
              {passwordPrompt.error}
            </div>
          )}

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '16px' }}>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={cancelPasswordPrompt}
              disabled={isSubmitting}
            >
              İptal
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!password || isSubmitting}
            >
              {isSubmitting ? 'Doğrulanıyor...' : 'Belgeyi Aç'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
