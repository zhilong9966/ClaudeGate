import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, KillReport, ProcInfo, Report } from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (cfg: AppConfig) => invoke<void>("save_config", { cfg }),
  defaultConfig: () => invoke<AppConfig>("default_config"),
  configPath: () => invoke<string>("config_path"),
  runChecks: () => invoke<Report>("run_checks"),
  launch: () => invoke<string>("launch_claude"),
  kill: () => invoke<KillReport>("kill_claude"),
  listProcesses: () => invoke<ProcInfo[]>("list_claude_processes"),
};
