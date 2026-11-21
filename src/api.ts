import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, DbStatus, KillReport, ProcInfo, Report } from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (cfg: AppConfig) => invoke<void>("save_config", { cfg }),
  defaultConfig: () => invoke<AppConfig>("default_config"),
  configPath: () => invoke<string>("config_path"),
  runChecks: (appId: string) => invoke<Report>("run_checks", { appId }),
  launch: (appId: string) => invoke<string>("launch_app", { appId }),
  kill: (appId: string) => invoke<KillReport>("kill_app", { appId }),
  listProcesses: (appId: string) => invoke<ProcInfo[]>("list_app_processes", { appId }),
  geoDbStatus: () => invoke<DbStatus>("geo_db_status"),
  updateGeoDb: (force: boolean) => invoke<string[]>("update_geo_db", { force }),
  shortcutInfo: () => invoke<{ path: string; exists: boolean; target: string }>("shortcut_info"),
  createShortcut: () => invoke<{ path: string; exists: boolean; target: string }>("create_shortcut"),
};
