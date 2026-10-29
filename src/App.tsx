import React, { useState } from 'react';
import type { AnnotationKind } from './types';
import { ThemeProvider } from './context/ThemeContext';
import { DocumentProvider, useDocument } from './context/DocumentContext';
import { OperationProvider } from './context/OperationContext';
import { Header } from './components/Header';
import { ToolRail } from './components/ToolRail';
import { Viewer } from './components/Viewer';
import { EmptyState } from './components/EmptyState';
import { ToolPanel } from './components/ToolPanel';
import { SearchDrawer } from './components/SearchDrawer';
import { PasswordModal } from './components/PasswordModal';
import { OperationBanner } from './components/OperationBanner';

const MainLayout: React.FC = () => {
  const { activeTab } = useDocument();

  // Annotation interactive state shared between ToolPanel and Viewer/CanvasOverlay
  const [annotationKind, setAnnotationKind] = useState<AnnotationKind>('highlight');
  const [annotationText, setAnnotationText] = useState<string>('Not');
  const [signatureImagePath, setSignatureImagePath] = useState<string>('');

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: '100vw',
        height: '100vh',
        overflow: 'hidden',
        position: 'relative',
      }}
    >
      {/* Top Application Bar with tabs & quick actions */}
      <Header />

      {/* Main Workspace: Left Tool Rail + Stage + Right Tool Form */}
      <div
        style={{
          display: 'flex',
          flex: 1,
          height: 'calc(100vh - 46px)',
          overflow: 'hidden',
          position: 'relative',
        }}
      >
        <ToolRail />

        {/* Center Stage: Document Viewer or Empty State */}
        {activeTab ? (
          <Viewer
            annotationKind={annotationKind}
            annotationText={annotationText}
            signatureImagePath={signatureImagePath}
          />
        ) : (
          <EmptyState />
        )}

        {/* Right Tool Parameters Panel */}
        <ToolPanel
          annotationKind={annotationKind}
          setAnnotationKind={setAnnotationKind}
          annotationText={annotationText}
          setAnnotationText={setAnnotationText}
          signatureImagePath={signatureImagePath}
          setSignatureImagePath={setSignatureImagePath}
        />
      </div>

      {/* Global Modals & Drawers */}
      <SearchDrawer />
      <PasswordModal />
      <OperationBanner />
    </div>
  );
};

export const App: React.FC = () => {
  return (
    <ThemeProvider>
      <DocumentProvider>
        <OperationProvider>
          <MainLayout />
        </OperationProvider>
      </DocumentProvider>
    </ThemeProvider>
  );
};

export default App;
