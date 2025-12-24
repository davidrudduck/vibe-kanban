import { TerminalContainer } from '@/components/terminal';

export function TerminalTest() {
  // Use a common working directory for testing
  const workingDir = '/tmp';

  return (
    <div className="flex flex-col h-full p-4">
      <h1 className="text-2xl font-bold mb-4">Terminal Test</h1>
      <div className="flex-1 rounded-lg border overflow-hidden relative" style={{ minHeight: '400px' }}>
        <TerminalContainer workingDir={workingDir} />
      </div>
    </div>
  );
}
