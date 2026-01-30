// Only export components that should NOT be code-split
export { SettingsLayout } from './SettingsLayout';
export { MobileSettingsAccordion } from './MobileSettingsAccordion';

// NOTE: Individual settings components (GeneralSettings, ProjectSettings, etc.)
// are NOT exported here to enable code splitting. Import them directly with
// lazy() from their individual files, e.g.:
//   const GeneralSettings = lazy(() => import('@/pages/settings/GeneralSettings'));
