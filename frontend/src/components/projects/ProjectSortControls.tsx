import { useTranslation } from 'react-i18next';
import { ArrowDownAZ, ArrowUpZA, Clock, History } from 'lucide-react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

export type SortOption =
  | 'name_asc'
  | 'name_desc'
  | 'recent_activity'
  | 'oldest_activity';

const STORAGE_KEY = 'project-sort-option';
const DEFAULT_SORT: SortOption = 'name_asc';

export function loadSortOption(): SortOption {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (
      saved &&
      ['name_asc', 'name_desc', 'recent_activity', 'oldest_activity'].includes(
        saved
      )
    ) {
      return saved as SortOption;
    }
    return DEFAULT_SORT;
  } catch {
    return DEFAULT_SORT;
  }
}

export function saveSortOption(option: SortOption): void {
  try {
    localStorage.setItem(STORAGE_KEY, option);
  } catch {
    // Ignore storage errors
  }
}

type Props = {
  value: SortOption;
  onChange: (option: SortOption) => void;
};

const sortOptions: Array<{
  value: SortOption;
  labelKey: string;
  icon: React.ElementType;
}> = [
  { value: 'name_asc', labelKey: 'sort.nameAsc', icon: ArrowDownAZ },
  { value: 'name_desc', labelKey: 'sort.nameDesc', icon: ArrowUpZA },
  { value: 'recent_activity', labelKey: 'sort.recentActivity', icon: Clock },
  { value: 'oldest_activity', labelKey: 'sort.oldestActivity', icon: History },
];

function ProjectSortControls({ value, onChange }: Props) {
  const { t } = useTranslation('projects');

  const handleChange = (newValue: string) => {
    const option = newValue as SortOption;
    onChange(option);
    saveSortOption(option);
  };

  return (
    <Select value={value} onValueChange={handleChange}>
      <SelectTrigger className="w-[180px] h-9 rounded-md bg-background">
        <SelectValue placeholder={t('sort.label')} />
      </SelectTrigger>
      <SelectContent>
        {sortOptions.map((option) => {
          const Icon = option.icon;
          return (
            <SelectItem key={option.value} value={option.value}>
              <div className="flex items-center gap-2">
                <Icon className="h-4 w-4" />
                <span>{t(option.labelKey)}</span>
              </div>
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}

export default ProjectSortControls;
