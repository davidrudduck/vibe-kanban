import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ReactNode } from 'react';
import type { ApprovalStatus, ToolStatus, Question } from 'shared/types';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { approvalsApi } from '@/lib/api';
import { Check, X, Send } from 'lucide-react';
import { Textarea } from '@/components/ui/textarea';
import { FileSearchTextarea } from '@/components/ui/file-search-textarea';

import { useHotkeysContext } from 'react-hotkeys-hook';
import { TabNavContext } from '@/contexts/TabNavigationContext';
import { useKeyApproveRequest, useKeyDenyApproval, Scope } from '@/keyboard';
import { useProject } from '@/contexts/ProjectContext';
import { useApprovalForm } from '@/contexts/ApprovalFormContext';

const DEFAULT_DENIAL_REASON = 'User denied this tool use request.';

// ---------- Types ----------
interface PendingApprovalEntryProps {
  pendingStatus: Extract<ToolStatus, { status: 'pending_approval' }>;
  executionProcessId?: string;
  children: ReactNode;
  toolName?: string;
  toolArguments?: unknown;
}

// Type for AskUserQuestion arguments
interface AskUserQuestionArgs {
  questions?: Array<{
    question: string;
    header?: string;
    multiSelect?: boolean;
    options?: Array<{
      label: string;
      description?: string;
    }>;
  }>;
}

// Extract and normalize questions from AskUserQuestion tool arguments
function extractQuestionsFromArgs(
  args: AskUserQuestionArgs | null | undefined
): Question[] | null {
  if (!args?.questions || !Array.isArray(args.questions)) {
    return null;
  }

  const questions: Question[] = [];
  for (const q of args.questions) {
    // Validate required fields
    if (!q.question || typeof q.question !== 'string') {
      continue;
    }
    // header defaults to question text if not provided
    const header = q.header || q.question;
    // options must be an array with at least one option
    if (!q.options || !Array.isArray(q.options) || q.options.length === 0) {
      continue;
    }
    // Normalize options to have required description field
    const options = q.options.map((opt) => ({
      label: opt.label || '',
      description: opt.description || '',
    }));
    if (!options.every((opt) => opt.label)) {
      continue;
    }
    questions.push({
      question: q.question,
      header,
      multiSelect: q.multiSelect ?? false,
      options,
    });
  }

  return questions.length > 0 ? questions : null;
}

function useApprovalCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date,
  paused: boolean
) {
  const totalSeconds = useMemo(() => {
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);

    return () => window.clearInterval(id);
  }, [timeoutAt, paused]);

  const percent = useMemo(
    () =>
      Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100))),
    [timeLeft, totalSeconds]
  );

  return { timeLeft, percent };
}

function ActionButtons({
  disabled,
  isResponding,
  onApprove,
  onStartDeny,
}: {
  disabled: boolean;
  isResponding: boolean;
  onApprove: () => void;
  onStartDeny: () => void;
}) {
  return (
    <div className="flex items-center gap-1.5 pr-4">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={onApprove}
            variant="ghost"
            className="h-8 w-8 rounded-full p-0"
            disabled={disabled}
            aria-label={isResponding ? 'Submitting approval' : 'Approve'}
            aria-busy={isResponding}
          >
            <Check className="h-5 w-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          <p>{isResponding ? 'Submitting…' : 'Approve request'}</p>
        </TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={onStartDeny}
            variant="ghost"
            className="h-8 w-8 rounded-full p-0"
            disabled={disabled}
            aria-label={isResponding ? 'Submitting denial' : 'Deny'}
            aria-busy={isResponding}
          >
            <X className="h-5 w-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          <p>{isResponding ? 'Submitting…' : 'Provide denial reason'}</p>
        </TooltipContent>
      </Tooltip>
    </div>
  );
}

function DenyReasonForm({
  isResponding,
  value,
  onChange,
  onCancel,
  onSubmit,
  inputRef,
  projectId,
}: {
  isResponding: boolean;
  value: string;
  onChange: (v: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
  inputRef: React.RefObject<HTMLTextAreaElement>;
  projectId?: string;
}) {
  return (
    <div className="mt-3 bg-background px-3 py-3 text-sm">
      <FileSearchTextarea
        ref={inputRef}
        value={value}
        onChange={onChange}
        placeholder="Let the agent know why this request was denied... Type @ to insert tags or search files."
        disabled={isResponding}
        className="w-full bg-transparent border px-3 py-2 text-sm resize-none min-h-[80px] focus-visible:outline-none"
        projectId={projectId}
      />
      <div className="mt-3 flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={isResponding}
        >
          Cancel
        </Button>
        <Button size="sm" onClick={onSubmit} disabled={isResponding}>
          Deny
        </Button>
      </div>
    </div>
  );
}

// ---------- Question Form for AskUserQuestion ----------
interface QuestionFormProps {
  questions: Question[];
  isResponding: boolean;
  disabled: boolean;
  answers: Record<string, string | string[]>;
  otherTexts: Record<string, string>;
  onAnswerChange: (header: string, value: string | string[]) => void;
  onOtherTextChange: (header: string, value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}

function QuestionForm({
  questions,
  isResponding,
  disabled,
  answers,
  otherTexts,
  onAnswerChange,
  onOtherTextChange,
  onSubmit,
  onCancel,
}: QuestionFormProps) {
  // Auto-submit: If this is the last question, it's single-select, and user selected a non-Other option
  const isLastQuestion = questions.length === 1;
  const lastQuestion = questions[0];
  const canAutoSubmit =
    isLastQuestion && lastQuestion && !lastQuestion.multiSelect;

  const handleOptionClick = (
    question: Question,
    optionLabel: string,
    isOther: boolean
  ) => {
    if (disabled) return;

    if (question.multiSelect) {
      // Multi-select: toggle the option
      const currentAnswers = (answers[question.header] as string[]) || [];
      const isSelected = currentAnswers.includes(optionLabel);
      if (isSelected) {
        onAnswerChange(
          question.header,
          currentAnswers.filter((a) => a !== optionLabel)
        );
      } else {
        onAnswerChange(question.header, [...currentAnswers, optionLabel]);
      }
    } else {
      // Single-select: set the option
      onAnswerChange(question.header, optionLabel);

      // Auto-submit if this is the last question and not "Other"
      if (canAutoSubmit && !isOther) {
        // Use setTimeout to allow state to update first
        setTimeout(() => onSubmit(), 0);
      }
    }
  };

  return (
    <div className="mt-3 bg-background px-3 py-3 text-sm space-y-4">
      {questions.map((question) => {
        const currentAnswer = answers[question.header];
        const isOtherSelected = question.multiSelect
          ? ((currentAnswer as string[]) || []).includes('Other')
          : currentAnswer === 'Other';
        const otherText = otherTexts[question.header] || '';

        return (
          <div key={question.header} className="space-y-2">
            <div className="font-medium text-foreground">{question.question}</div>
            <div className="flex flex-wrap gap-2">
              {question.options.map((option) => {
                const isSelected = question.multiSelect
                  ? ((currentAnswer as string[]) || []).includes(option.label)
                  : currentAnswer === option.label;

                return (
                  <Tooltip key={option.label}>
                    <TooltipTrigger asChild>
                      <Button
                        variant={isSelected ? 'default' : 'outline'}
                        size="sm"
                        onClick={() =>
                          handleOptionClick(question, option.label, false)
                        }
                        disabled={disabled}
                        className="h-auto py-1.5 px-3"
                      >
                        {option.label}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom" className="max-w-xs">
                      <p>{option.description}</p>
                    </TooltipContent>
                  </Tooltip>
                );
              })}

              {/* "Other" option - always available per spec */}
              <Button
                variant={isOtherSelected ? 'default' : 'outline'}
                size="sm"
                onClick={() => handleOptionClick(question, 'Other', true)}
                disabled={disabled}
                className="h-auto py-1.5 px-3"
              >
                Other
              </Button>
            </div>

            {/* Other text input - shown when "Other" is selected */}
            {isOtherSelected && (
              <Textarea
                value={otherText}
                onChange={(e) =>
                  onOtherTextChange(question.header, e.target.value)
                }
                placeholder="Please specify..."
                disabled={disabled}
                className="w-full bg-transparent border px-3 py-2 text-sm resize-none min-h-[60px] mt-2"
              />
            )}
          </div>
        );
      })}

      <div className="flex flex-wrap items-center justify-end gap-2 pt-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={isResponding}
        >
          Cancel
        </Button>
        <Button
          size="sm"
          onClick={onSubmit}
          disabled={isResponding || disabled}
        >
          <Send className="h-4 w-4 mr-1.5" />
          Submit
        </Button>
      </div>
    </div>
  );
}

// ---------- Main Component ----------
const PendingApprovalEntry = ({
  pendingStatus,
  executionProcessId,
  children,
  toolName,
  toolArguments,
}: PendingApprovalEntryProps) => {
  // Debug logging for AskUserQuestion flow
  console.debug('[PendingApprovalEntry] Rendering with:', {
    approvalId: pendingStatus.approval_id,
    toolName,
    hasToolArguments: toolArguments !== undefined && toolArguments !== null,
    toolArgumentsType: toolArguments === null ? 'null' : typeof toolArguments,
    isAskUserQuestion: toolName === 'AskUserQuestion',
  });

  // If this is AskUserQuestion, log more details about the questions
  if (toolName === 'AskUserQuestion') {
    const args = toolArguments as AskUserQuestionArgs | undefined;
    console.debug('[PendingApprovalEntry] AskUserQuestion details:', {
      hasQuestions: args?.questions !== undefined,
      questionsIsArray: Array.isArray(args?.questions),
      questionsCount: args?.questions?.length ?? 0,
      questions: args?.questions,
    });
  }

  const [isResponding, setIsResponding] = useState(false);
  const [hasResponded, setHasResponded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // State for AskUserQuestion answers
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({});
  const [otherTexts, setOtherTexts] = useState<Record<string, string>>({});

  // Extract questions if this is an AskUserQuestion with valid questions
  const extractedQuestions = useMemo(() => {
    if (toolName !== 'AskUserQuestion') return null;
    const questions = extractQuestionsFromArgs(
      toolArguments as AskUserQuestionArgs | null | undefined
    );
    console.debug('[PendingApprovalEntry] Extracted questions:', {
      hasQuestions: questions !== null,
      questionsCount: questions?.length ?? 0,
    });
    return questions;
  }, [toolName, toolArguments]);

  const isAskUserQuestionWithQuestions = extractedQuestions !== null;

  const {
    isEnteringReason,
    denyReason,
    setIsEnteringReason,
    setDenyReason,
    clear,
  } = useApprovalForm(pendingStatus.approval_id);

  const denyReasonRef = useRef<HTMLTextAreaElement | null>(null);
  const { projectId } = useProject();

  const { enableScope, disableScope, activeScopes } = useHotkeysContext();
  const tabNav = useContext(TabNavContext);
  const isLogsTabActive = tabNav ? tabNav.activeTab === 'logs' : true;
  const dialogScopeActive = activeScopes.includes(Scope.DIALOG);
  const shouldControlScopes = isLogsTabActive && !dialogScopeActive;
  const approvalsScopeEnabledRef = useRef(false);
  const dialogScopeActiveRef = useRef(dialogScopeActive);

  useEffect(() => {
    dialogScopeActiveRef.current = dialogScopeActive;
  }, [dialogScopeActive]);

  const { timeLeft } = useApprovalCountdown(
    pendingStatus.requested_at,
    pendingStatus.timeout_at,
    hasResponded
  );

  const disabled = isResponding || hasResponded || timeLeft <= 0;

  const shouldEnableApprovalsScope = shouldControlScopes && !disabled;

  useEffect(() => {
    const shouldEnable = shouldEnableApprovalsScope;

    if (shouldEnable && !approvalsScopeEnabledRef.current) {
      enableScope(Scope.APPROVALS);
      disableScope(Scope.KANBAN);
      approvalsScopeEnabledRef.current = true;
    } else if (!shouldEnable && approvalsScopeEnabledRef.current) {
      disableScope(Scope.APPROVALS);
      if (!dialogScopeActive) {
        enableScope(Scope.KANBAN);
      }
      approvalsScopeEnabledRef.current = false;
    }

    return () => {
      if (approvalsScopeEnabledRef.current) {
        disableScope(Scope.APPROVALS);
        if (!dialogScopeActiveRef.current) {
          enableScope(Scope.KANBAN);
        }
        approvalsScopeEnabledRef.current = false;
      }
    };
  }, [
    disableScope,
    enableScope,
    dialogScopeActive,
    shouldEnableApprovalsScope,
  ]);

  const respond = useCallback(
    async (
      approved: boolean,
      reason?: string,
      answersPayload?: Record<string, string>
    ) => {
      if (disabled) return;
      if (!executionProcessId) {
        setError('Missing executionProcessId');
        return;
      }

      setIsResponding(true);
      setError(null);

      const status: ApprovalStatus = approved
        ? { status: 'approved' }
        : { status: 'denied', reason };

      try {
        await approvalsApi.respond(pendingStatus.approval_id, {
          execution_process_id: executionProcessId,
          status,
          answers: answersPayload,
        });
        setHasResponded(true);
        clear();
      } catch (e: unknown) {
        console.error('Approval respond failed:', e);
        const errorMessage =
          e instanceof Error ? e.message : 'Failed to send response';
        setError(errorMessage);
      } finally {
        setIsResponding(false);
      }
    },
    [disabled, executionProcessId, pendingStatus.approval_id, clear]
  );

  const handleApprove = useCallback(() => respond(true), [respond]);
  const handleStartDeny = useCallback(() => {
    if (disabled) return;
    setError(null);
    setIsEnteringReason(true);
  }, [disabled, setIsEnteringReason]);

  const handleCancelDeny = useCallback(() => {
    if (isResponding) return;
    clear();
  }, [isResponding, clear]);

  const handleSubmitDeny = useCallback(() => {
    const trimmed = denyReason.trim();
    respond(false, trimmed || DEFAULT_DENIAL_REASON);
  }, [denyReason, respond]);

  // Handlers for AskUserQuestion form
  const handleAnswerChange = useCallback(
    (header: string, value: string | string[]) => {
      setAnswers((prev) => ({ ...prev, [header]: value }));
    },
    []
  );

  const handleOtherTextChange = useCallback((header: string, value: string) => {
    setOtherTexts((prev) => ({ ...prev, [header]: value }));
  }, []);

  const buildFinalAnswers = useCallback((): Record<string, string> => {
    if (!extractedQuestions) return {};
    const result: Record<string, string> = {};
    for (const question of extractedQuestions) {
      const answer = answers[question.header];
      if (question.multiSelect) {
        const selectedOptions = (answer as string[]) || [];
        // Replace "Other" with the actual text if provided
        const finalOptions = selectedOptions.map((opt) =>
          opt === 'Other' ? otherTexts[question.header] || 'Other' : opt
        );
        result[question.header] = finalOptions.join(', ');
      } else {
        const selectedOption = answer as string;
        if (selectedOption === 'Other') {
          result[question.header] = otherTexts[question.header] || 'Other';
        } else {
          result[question.header] = selectedOption || '';
        }
      }
    }
    return result;
  }, [answers, otherTexts, extractedQuestions]);

  const handleQuestionSubmit = useCallback(() => {
    const finalAnswers = buildFinalAnswers();
    console.debug('[PendingApprovalEntry] Submitting question answers:', finalAnswers);
    respond(true, undefined, finalAnswers);
  }, [buildFinalAnswers, respond]);

  const handleQuestionCancel = useCallback(() => {
    console.debug('[PendingApprovalEntry] Question cancelled');
    respond(false, 'User cancelled');
  }, [respond]);

  const triggerDeny = useCallback(
    (event?: KeyboardEvent) => {
      if (!isEnteringReason || disabled || hasResponded) return;
      event?.preventDefault();
      handleSubmitDeny();
    },
    [isEnteringReason, disabled, hasResponded, handleSubmitDeny]
  );

  useKeyApproveRequest(handleApprove, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && !isEnteringReason,
    preventDefault: true,
  });

  useKeyDenyApproval(triggerDeny, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && !hasResponded,
    enableOnFormTags: ['textarea', 'TEXTAREA'],
    preventDefault: true,
  });

  useEffect(() => {
    if (!isEnteringReason) return;
    const id = window.setTimeout(() => denyReasonRef.current?.focus(), 0);
    return () => window.clearTimeout(id);
  }, [isEnteringReason]);

  return (
    <div className="relative mt-3">
      <div className="overflow-hidden border">
        {children}

        <div className="border-t bg-background px-2 py-1.5 text-xs sm:text-sm">
          <TooltipProvider>
            {/* AskUserQuestion with valid questions: show QuestionForm */}
            {isAskUserQuestionWithQuestions && !hasResponded && (
              <QuestionForm
                questions={extractedQuestions}
                isResponding={isResponding}
                disabled={disabled}
                answers={answers}
                otherTexts={otherTexts}
                onAnswerChange={handleAnswerChange}
                onOtherTextChange={handleOtherTextChange}
                onSubmit={handleQuestionSubmit}
                onCancel={handleQuestionCancel}
              />
            )}

            {/* Response submitted message for AskUserQuestion */}
            {isAskUserQuestionWithQuestions && hasResponded && (
              <div className="text-muted-foreground text-center py-2">
                Response submitted
              </div>
            )}

            {/* Generic approve/deny UI for non-AskUserQuestion tools */}
            {!isAskUserQuestionWithQuestions && (
              <>
                <div className="flex items-center justify-between gap-1.5 pl-4">
                  <div className="flex items-center gap-1.5">
                    {!isEnteringReason && (
                      <span className="text-muted-foreground">
                        Would you like to approve this?
                      </span>
                    )}
                  </div>
                  {!isEnteringReason && (
                    <ActionButtons
                      disabled={disabled}
                      isResponding={isResponding}
                      onApprove={handleApprove}
                      onStartDeny={handleStartDeny}
                    />
                  )}
                </div>

                {isEnteringReason && !hasResponded && (
                  <DenyReasonForm
                    isResponding={isResponding}
                    value={denyReason}
                    onChange={setDenyReason}
                    onCancel={handleCancelDeny}
                    onSubmit={handleSubmitDeny}
                    inputRef={denyReasonRef}
                    projectId={projectId}
                  />
                )}
              </>
            )}

            {error && (
              <div
                className="mt-1 text-xs text-red-600"
                role="alert"
                aria-live="polite"
              >
                {error}
              </div>
            )}
          </TooltipProvider>
        </div>
      </div>
    </div>
  );
};

export default PendingApprovalEntry;
