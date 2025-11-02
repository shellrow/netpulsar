import { toDate } from "../types/net.ts";

// Format helpers
export function nv(v?: string | number | null): string {
  if (v === null || v === undefined) return "-";
  const s = String(v).trim();
  return s.length ? s : "-";
}

export function fmtIfType(t?: string) {
  return t ?? "Unknown";
}

export function fmtDate(ts: any) {
  const d = toDate(ts);
  return isNaN(+d) ? "-" : d.toLocaleString();
}

export function hexFlags(flags?: number) {
  if (flags == null) return "0x0";
  return "0x" + flags.toString(16).toUpperCase();
}

export function severityByOper(s?: string) {
  const v = (s ?? "").toLowerCase();
  return v === "up" ? "success" : v === "down" ? "danger" : "secondary";
}
