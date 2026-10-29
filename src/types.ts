export interface DocumentInfo {
  id: string;
  path: string;
  name: string;
  pages: number;
  size: number;
  revision: string;
  /** [width, height] in PDF points per page, used for lazy layout. */
  pageSizes?: [number, number][];
}

export interface RenderedPage {
  dataUrl: string;
  width: number;
  height: number;
}

export interface SearchHit {
  page: number;
  text: string;
}

export interface RedactRect {
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type AnnotationKind = 'highlight' | 'text' | 'ink' | 'signature';

export interface Annotation {
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  kind: AnnotationKind;
  text?: string;
  points?: [number, number][];
  imagePath?: string;
}

export type OperationKind =
  | 'merge'
  | 'extract'
  | 'split'
  | 'remove'
  | 'reorder'
  | 'rotate'
  | 'watermark'
  | 'number'
  | 'protect'
  | 'unlock'
  | 'compress'
  | 'images-to-pdf'
  | 'pdf-to-images'
  | 'redact'
  | 'annotate'
  | 'extract-text'
  | 'ocr'
  | 'pdf-to-word'
  | 'word-to-pdf'
  | 'image-to-word'
  | 'install-ocr';

export interface OperationRequest {
  id: string;
  kind: OperationKind;
  inputs: string[];
  output: string;
  pages?: string;
  password?: string;
  inputPasswords?: Record<string, string>;
  newPassword?: string;
  text?: string;
  rotation?: number;
  dpi?: number;
  imagePaths?: string[];
  rects?: RedactRect[];
  annotations?: Annotation[];
  ocr?: boolean;
  downloadUrl?: string;
  sha256?: string;
  targetDir?: string;
}

export interface OperationResult {
  outputs: string[];
  message: string;
}

export interface Progress {
  id: string;
  completed: number;
  total: number;
  message: string;
}

export interface TabState {
  info: DocumentInfo;
  /** Advances on every explicit reopen, even when file metadata is unchanged. */
  renderGeneration: number;
  currentPage: number;
  zoom: number; // 1.0 = 100%
  sourcePassword?: string;
  redactions: RedactRect[];
  annotations: Annotation[];
}

export type Theme = 'origami' | 'folio-noir';
