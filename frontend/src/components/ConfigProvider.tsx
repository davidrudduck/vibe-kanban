import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import {
  type Config,
  type Environment,
  type UserSystemInfo,
  type BaseAgentCapability,
  type LoginStatus,
} from 'shared/types';
import type { ExecutorConfig } from 'shared/types';
import { configApi } from '../lib/api';
import { updateLanguageFromConfig } from '../i18n/config';
import { Loader } from '@/components/ui/loader';

interface UserSystemState {
  config: Config | null;
  environment: Environment | null;
  profiles: Record<string, ExecutorConfig> | null;
  capabilities: Record<string, BaseAgentCapability[]> | null;
  analyticsUserId: string | null;
  loginStatus: LoginStatus | null;
}

interface UserSystemContextType {
  // Full system state
  system: UserSystemState;

  // Hot path - config helpers (most frequently used)
  config: Config | null;
  updateConfig: (updates: Partial<Config>) => void;
  updateAndSaveConfig: (updates: Partial<Config>) => Promise<boolean>;
  saveConfig: () => Promise<boolean>;

  // System data access
  environment: Environment | null;
  profiles: Record<string, ExecutorConfig> | null;
  capabilities: Record<string, BaseAgentCapability[]> | null;
  analyticsUserId: string | null;
  loginStatus: LoginStatus | null;
  setEnvironment: (env: Environment | null) => void;
  setProfiles: (profiles: Record<string, ExecutorConfig> | null) => void;
  setCapabilities: (caps: Record<string, BaseAgentCapability[]> | null) => void;

  // Reload system data
  reloadSystem: () => Promise<void>;

  // State
  loading: boolean;
  connecting: boolean; // True when waiting for backend to start
}

const UserSystemContext = createContext<UserSystemContextType | undefined>(
  undefined
);

interface UserSystemProviderProps {
  children: ReactNode;
}

// Retry configuration for waiting on backend startup
const INITIAL_RETRY_DELAY_MS = 500;
const MAX_RETRY_DELAY_MS = 5000;
const MAX_RETRIES = 30; // ~30 seconds with backoff

export function UserSystemProvider({ children }: UserSystemProviderProps) {
  // Split state for performance - independent re-renders
  const [config, setConfig] = useState<Config | null>(null);
  const [environment, setEnvironment] = useState<Environment | null>(null);
  const [profiles, setProfiles] = useState<Record<
    string,
    ExecutorConfig
  > | null>(null);
  const [capabilities, setCapabilities] = useState<Record<
    string,
    BaseAgentCapability[]
  > | null>(null);
  const [analyticsUserId, setAnalyticsUserId] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<LoginStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const loadUserSystem = async () => {
      let retries = 0;
      let delay = INITIAL_RETRY_DELAY_MS;

      while (!cancelled && retries < MAX_RETRIES) {
        try {
          const userSystemInfo: UserSystemInfo = await configApi.getConfig();
          if (cancelled) return;

          setConfig(userSystemInfo.config);
          setEnvironment(userSystemInfo.environment);
          setAnalyticsUserId(userSystemInfo.analytics_user_id);
          setLoginStatus(userSystemInfo.login_status);
          setProfiles(
            userSystemInfo.executors as Record<string, ExecutorConfig> | null
          );
          setCapabilities(
            (userSystemInfo.capabilities || null) as Record<
              string,
              BaseAgentCapability[]
            > | null
          );
          setConnecting(false);
          setLoading(false);
          return; // Success - exit the retry loop
        } catch (err) {
          // Check if this is a connection/proxy error (backend not ready yet)
          const errorMessage = err instanceof Error ? err.message : '';
          const errorStatus =
            err && typeof err === 'object' && 'status' in err
              ? (err as { status?: number }).status
              : undefined;
          const isProxyError =
            errorStatus === 502 || errorStatus === 503 || errorStatus === 504;
          const isNetworkError =
            err instanceof TypeError ||
            errorMessage.includes('Failed to fetch') ||
            errorMessage.includes('NetworkError') ||
            errorMessage.includes('ECONNREFUSED');
          const isConnectionError = isNetworkError || isProxyError;

          if (isConnectionError && retries < MAX_RETRIES - 1) {
            setConnecting(true);
            retries++;
            await new Promise((resolve) =>
              setTimeout(resolve, delay + Math.random() * 100)
            );
            delay = Math.min(delay * 1.5, MAX_RETRY_DELAY_MS);
          } else {
            // Non-connection error or max retries reached
            console.error('Error loading user system:', err);
            setConnecting(false);
            setLoading(false);
            return;
          }
        }
      }

      // Max retries exceeded
      if (!cancelled) {
        console.error('Failed to connect to backend after maximum retries');
        setConnecting(false);
        setLoading(false);
      }
    };

    loadUserSystem();

    return () => {
      cancelled = true;
    };
  }, []);

  // Sync language with i18n when config changes
  useEffect(() => {
    if (config?.language) {
      updateLanguageFromConfig(config.language);
    }
  }, [config?.language]);

  const updateConfig = useCallback((updates: Partial<Config>) => {
    setConfig((prev) => (prev ? { ...prev, ...updates } : null));
  }, []);

  const saveConfig = useCallback(async (): Promise<boolean> => {
    if (!config) return false;
    try {
      await configApi.saveConfig(config);
      return true;
    } catch (err) {
      console.error('Error saving config:', err);
      return false;
    }
  }, [config]);

  const updateAndSaveConfig = useCallback(
    async (updates: Partial<Config>): Promise<boolean> => {
      setLoading(true);
      const newConfig: Config | null = config
        ? { ...config, ...updates }
        : null;
      try {
        if (!newConfig) return false;
        const saved = await configApi.saveConfig(newConfig);
        setConfig(saved);
        return true;
      } catch (err) {
        console.error('Error saving config:', err);
        return false;
      } finally {
        setLoading(false);
      }
    },
    [config]
  );

  const reloadSystem = useCallback(async () => {
    setLoading(true);
    try {
      const userSystemInfo: UserSystemInfo = await configApi.getConfig();
      setConfig(userSystemInfo.config);
      setEnvironment(userSystemInfo.environment);
      setAnalyticsUserId(userSystemInfo.analytics_user_id);
      setLoginStatus(userSystemInfo.login_status);
      setProfiles(
        userSystemInfo.executors as Record<string, ExecutorConfig> | null
      );
      setCapabilities(
        (userSystemInfo.capabilities || null) as Record<
          string,
          BaseAgentCapability[]
        > | null
      );
    } catch (err) {
      console.error('Error reloading user system:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  // Memoize context value to prevent unnecessary re-renders
  const value = useMemo<UserSystemContextType>(
    () => ({
      system: {
        config,
        environment,
        profiles,
        capabilities,
        analyticsUserId,
        loginStatus,
      },
      config,
      environment,
      profiles,
      capabilities,
      analyticsUserId,
      loginStatus,
      updateConfig,
      saveConfig,
      updateAndSaveConfig,
      setEnvironment,
      setProfiles,
      setCapabilities,
      reloadSystem,
      loading,
      connecting,
    }),
    [
      config,
      environment,
      profiles,
      capabilities,
      analyticsUserId,
      loginStatus,
      updateConfig,
      saveConfig,
      updateAndSaveConfig,
      reloadSystem,
      loading,
      connecting,
    ]
  );

  // Gate children until backend is ready - prevents child components from
  // making API calls before the backend is available
  if (loading || connecting) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <Loader
          message={connecting ? 'Starting server...' : 'Loading...'}
          size={32}
        />
      </div>
    );
  }

  return (
    <UserSystemContext.Provider value={value}>
      {children}
    </UserSystemContext.Provider>
  );
}

export function useUserSystem() {
  const context = useContext(UserSystemContext);
  if (context === undefined) {
    throw new Error('useUserSystem must be used within a UserSystemProvider');
  }
  return context;
}
