import { useState } from 'react';
import type { AskUserQuestionItem } from 'shared/types';
import { Button } from '@vibe/ui/components/Button';

interface QuestionFormProps {
  questions: AskUserQuestionItem[];
  onSubmit: (answers: Record<string, string[]>) => void;
  disabled?: boolean;
  isResponding?: boolean;
}

function QuestionField({
  question,
  value,
  onChange,
}: {
  question: AskUserQuestionItem;
  value: string[];
  onChange: (vals: string[]) => void;
}) {
  const { question: label, options, multiSelect } = question;

  if (options.length === 0) {
    return (
      <div className="flex flex-col gap-1">
        <label className="text-sm font-medium">{label}</label>
        <input
          type="text"
          className="border-border bg-background text-foreground rounded border px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
          value={value[0] ?? ''}
          onChange={(e) => onChange([e.target.value])}
        />
      </div>
    );
  }

  if (multiSelect) {
    return (
      <div className="flex flex-col gap-1">
        <label className="text-sm font-medium">{label}</label>
        <div className="flex flex-col gap-1 pl-1">
          {options.map((opt) => {
            const checked = value.includes(opt.label);
            return (
              <label
                key={opt.label}
                className="flex cursor-pointer items-start gap-2 text-sm"
              >
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={checked}
                  onChange={(e) => {
                    if (e.target.checked) {
                      onChange([...value, opt.label]);
                    } else {
                      onChange(value.filter((v) => v !== opt.label));
                    }
                  }}
                />
                <span>
                  <span className="font-medium">{opt.label}</span>
                  {opt.description && (
                    <span className="text-muted-foreground ml-1">
                      — {opt.description}
                    </span>
                  )}
                </span>
              </label>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm font-medium">{label}</label>
      <div className="flex flex-col gap-1 pl-1">
        {options.map((opt) => (
          <label
            key={opt.label}
            className="flex cursor-pointer items-start gap-2 text-sm"
          >
            <input
              type="radio"
              className="mt-0.5"
              checked={value[0] === opt.label}
              onChange={() => onChange([opt.label])}
            />
            <span>
              <span className="font-medium">{opt.label}</span>
              {opt.description && (
                <span className="text-muted-foreground ml-1">
                  — {opt.description}
                </span>
              )}
            </span>
          </label>
        ))}
      </div>
    </div>
  );
}

const QuestionForm = ({
  questions,
  onSubmit,
  disabled = false,
  isResponding = false,
}: QuestionFormProps) => {
  const [answers, setAnswers] = useState<Record<string, string[]>>(() =>
    Object.fromEntries(questions.map((q) => [q.header, []]))
  );

  const handleChange = (header: string, vals: string[]) => {
    setAnswers((prev) => ({ ...prev, [header]: vals }));
  };

  const allAnswered = questions.every(
    (q) => (answers[q.header] ?? []).length > 0
  );

  const handleSubmit = () => {
    if (!allAnswered || isResponding) return;
    onSubmit(answers);
  };

  return (
    <div className="flex flex-col gap-4 p-4">
      {questions.map((q) => (
        <QuestionField
          key={q.header}
          question={q}
          value={answers[q.header] ?? []}
          onChange={(vals) => handleChange(q.header, vals)}
        />
      ))}
      <div className="flex justify-end">
        <Button
          size="sm"
          onClick={handleSubmit}
          disabled={disabled || !allAnswered || isResponding}
        >
          {isResponding ? 'Submitting…' : 'Submit'}
        </Button>
      </div>
    </div>
  );
};

export default QuestionForm;
