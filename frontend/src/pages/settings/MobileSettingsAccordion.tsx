import { useState, useMemo, useCallback, Suspense, lazy } from 'react';
import { useTranslation } from 'react-i18next';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Settings,
  Cpu,
  Server,
  FolderOpen,
  Building2,
  Database,
  ChevronDown,
  Search,
  X,
  Loader2,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';

// Lazy load settings components to improve initial load time
const GeneralSettings = lazy(() =>
  import('./GeneralSettings').then((m) => ({ default: m.GeneralSettings }))
);
const ProjectSettings = lazy(() =>
  import('./ProjectSettings').then((m) => ({ default: m.ProjectSettings }))
);
const OrganizationSettings = lazy(() =>
  import('./OrganizationSettings').then((m) => ({ default: m.OrganizationSettings }))
);
const AgentSettings = lazy(() =>
  import('./AgentSettings').then((m) => ({ default: m.AgentSettings }))
);
const McpSettings = lazy(() =>
  import('./McpSettings').then((m) => ({ default: m.McpSettings }))
);
const BackupSettings = lazy(() =>
  import('./BackupSettings').then((m) => ({ default: m.BackupSettings }))
);

interface SettingsSection {
  id: string;
  path: string;
  icon: LucideIcon;
  component: React.LazyExoticComponent<React.ComponentType>;
}

const settingsSections: SettingsSection[] = [
  {
    id: 'general',
    path: 'general',
    icon: Settings,
    component: GeneralSettings,
  },
  {
    id: 'projects',
    path: 'projects',
    icon: FolderOpen,
    component: ProjectSettings,
  },
  {
    id: 'organizations',
    path: 'organizations',
    icon: Building2,
    component: OrganizationSettings,
  },
  {
    id: 'agents',
    path: 'agents',
    icon: Cpu,
    component: AgentSettings,
  },
  {
    id: 'mcp',
    path: 'mcp',
    icon: Server,
    component: McpSettings,
  },
  {
    id: 'backups',
    path: 'backups',
    icon: Database,
    component: BackupSettings,
  },
];

function LoadingFallback() {
  return (
    <div className="flex items-center justify-center py-12">
      <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
    </div>
  );
}

interface AccordionItemProps {
  section: SettingsSection;
  isExpanded: boolean;
  onToggle: () => void;
  title: string;
  description: string;
}

function AccordionItem({
  section,
  isExpanded,
  onToggle,
  title,
  description,
}: AccordionItemProps) {
  const Icon = section.icon;
  const Component = section.component;

  return (
    <div className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={onToggle}
        className={cn(
          'w-full flex items-start gap-3 px-4 py-4 text-left',
          'hover:bg-muted/50 transition-colors',
          'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset',
          isExpanded && 'bg-muted/30'
        )}
        aria-expanded={isExpanded}
        aria-controls={`settings-section-${section.id}`}
      >
        <Icon
          className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground"
          data-testid="section-icon"
        />
        <div className="flex-1 min-w-0">
          <div className="font-medium">{title}</div>
          <div className="text-sm text-muted-foreground" data-testid="section-description">
            {description}
          </div>
        </div>
        <ChevronDown
          className={cn(
            'h-5 w-5 shrink-0 text-muted-foreground transition-transform duration-200',
            isExpanded && 'rotate-180'
          )}
          data-testid="chevron-icon"
        />
      </button>

      <AnimatePresence initial={false}>
        {isExpanded && (
          <motion.div
            id={`settings-section-${section.id}`}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: 'easeInOut' }}
            className="overflow-hidden"
          >
            <div className="px-4 pb-4">
              <Suspense fallback={<LoadingFallback />}>
                <Component />
              </Suspense>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export function MobileSettingsAccordion() {
  const { t } = useTranslation('settings');
  const [expandedSection, setExpandedSection] = useState<string | null>('general');
  const [searchQuery, setSearchQuery] = useState('');

  const handleToggle = useCallback((sectionId: string) => {
    setExpandedSection((current) => (current === sectionId ? null : sectionId));
  }, []);

  const handleSearchChange = useCallback((value: string) => {
    setSearchQuery(value);
    // If searching, collapse all sections
    if (value) {
      setExpandedSection(null);
    }
  }, []);

  const handleClearSearch = useCallback(() => {
    setSearchQuery('');
    // Restore default expanded section
    setExpandedSection('general');
  }, []);

  const filteredSections = useMemo(() => {
    if (!searchQuery.trim()) {
      return settingsSections;
    }

    const query = searchQuery.toLowerCase();
    return settingsSections.filter((section) => {
      const title = t(`settings.layout.nav.${section.path}`).toLowerCase();
      const description = t(`settings.layout.nav.${section.path}Desc`).toLowerCase();
      return title.includes(query) || description.includes(query);
    });
  }, [searchQuery, t]);

  // Auto-expand the only result when filtering
  useMemo(() => {
    if (filteredSections.length === 1) {
      setExpandedSection(filteredSections[0].id);
    }
  }, [filteredSections]);

  return (
    <div className="flex flex-col h-full">
      {/* Search Bar */}
      <div className="sticky top-0 z-10 bg-background/95 backdrop-blur-sm border-b border-border px-4 py-3">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            type="search"
            placeholder={t('settings.layout.searchPlaceholder', 'Search settings...')}
            value={searchQuery}
            onChange={(e) => handleSearchChange(e.target.value)}
            className="pl-10 pr-10 h-10"
          />
          {searchQuery && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleClearSearch}
              className="absolute right-1 top-1/2 -translate-y-1/2 h-8 w-8 p-0"
              aria-label="Clear search"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>

      {/* Accordion Sections */}
      <div className="flex-1 overflow-auto">
        {filteredSections.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
            <Search className="h-8 w-8 text-muted-foreground mb-4" />
            <p className="text-muted-foreground">
              {t('settings.layout.noResults', 'No settings match your search')}
            </p>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleClearSearch}
              className="mt-2"
            >
              {t('settings.layout.clearSearch', 'Clear search')}
            </Button>
          </div>
        ) : (
          <div className="divide-y divide-border border-t border-border">
            {filteredSections.map((section) => (
              <AccordionItem
                key={section.id}
                section={section}
                isExpanded={expandedSection === section.id}
                onToggle={() => handleToggle(section.id)}
                title={t(`settings.layout.nav.${section.path}`)}
                description={t(`settings.layout.nav.${section.path}Desc`)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
