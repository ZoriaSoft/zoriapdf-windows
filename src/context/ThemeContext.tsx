import React, { createContext, useContext, useEffect, useState } from 'react';
import type { Theme } from '../types';

interface ThemeContextValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
  reducedMotion: boolean;
  setReducedMotion: (val: boolean) => void;
  toggleReducedMotion: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [theme, setThemeState] = useState<Theme>(() => {
    const saved = localStorage.getItem('zoriapdf_theme');
    return saved === 'folio-noir' ? 'folio-noir' : 'origami';
  });

  const [reducedMotion, setReducedMotionState] = useState<boolean>(() => {
    const saved = localStorage.getItem('zoriapdf_reduced_motion');
    if (saved !== null) return saved === 'true';
    if (typeof window !== 'undefined' && window.matchMedia) {
      return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    }
    return false;
  });

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('zoriapdf_theme', theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.setAttribute(
      'data-reduced-motion',
      reducedMotion ? 'true' : 'false'
    );
    localStorage.setItem('zoriapdf_reduced_motion', String(reducedMotion));
  }, [reducedMotion]);

  const setTheme = (t: Theme) => setThemeState(t);
  const toggleTheme = () => {
    setThemeState((prev) => (prev === 'origami' ? 'folio-noir' : 'origami'));
  };

  const setReducedMotion = (val: boolean) => setReducedMotionState(val);
  const toggleReducedMotion = () => setReducedMotionState((prev) => !prev);

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme,
        toggleTheme,
        reducedMotion,
        setReducedMotion,
        toggleReducedMotion,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
};

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error('useTheme must be used within ThemeProvider');
  }
  return ctx;
}
