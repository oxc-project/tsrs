export interface CheckOptions {
  /** A tsconfig file or directory, relative to cwd. Defaults to ".". */
  project?: string;
  /** Working directory, captured when called. Defaults to process.cwd(). */
  cwd?: string;
}

export interface DiagnosticPosition {
  line: number;
  character: number;
}

export interface Diagnostic {
  fileName?: string;
  /** UTF-16 offsets; -1 for a diagnostic without a source position. */
  pos: number;
  end: number;
  startPosition?: DiagnosticPosition;
  endPosition?: DiagnosticPosition;
  sourceLines?: { line: number; text: string }[];
  code: number;
  /** Warning = 0, Error = 1, Suggestion = 2, Message = 3. */
  category: number;
  text: string;
  source?: string;
  reportsUnnecessary?: boolean;
  reportsDeprecated?: boolean;
  messageChain?: Diagnostic[];
  relatedInformation?: Diagnostic[];
}

export interface CheckResult {
  exitCode: number;
  diagnostics: Diagnostic[];
  /** Non-diagnostic output, such as listFiles or traceResolution. */
  stdout: string;
  stderr: string;
}

/** Equivalent to tsrs --noEmit -p project. Compiler errors resolve with diagnostics and a nonzero exitCode. */
export function check(options?: CheckOptions): Promise<CheckResult>;
/** Synchronous check; blocks JavaScript until compilation completes. */
export function checkSync(options?: CheckOptions): CheckResult;
