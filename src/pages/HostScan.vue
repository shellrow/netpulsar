<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { HostScanProgress, HostScanReport, HostScanSetting } from "../types/probe";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";

const form = reactive({
  mode: "cidr" as "cidr" | "list",
  cidr: "",
  list: "192.168.1.1\n192.168.1.2",
  hop_limit: 64,
  timeout_ms: 1000,
  count: 1,
  payload: "np:hs",
  ordered: false,
  concurrency: 100,
});

const running = ref(false);
const loading = ref(false);
const err = ref<string | null>(null);
const progress = ref<HostScanProgress[]>([]);
const report = ref<HostScanReport | null>(null);

const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();

const MAX_EXPAND = 65536;

// Expand IPv4 CIDR like "192.168.1.0/24" to host list (excludes network/broadcast when prefix <= 30)
function expandIpv4Cidr(cidr: string, max = MAX_EXPAND): string[] {
  const m = cidr.trim().match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)\/(\d{1,2})$/);
  if (!m) return [];
  const [ , a, b, c, d, prefixStr ] = m;
  const prefix = parseInt(prefixStr, 10);
  if (prefix < 0 || prefix > 32) return [];

  const total = estimateHosts(cidr);
  if (total === 0 || total > max) return [];

  const ipNum =
    (parseInt(a,10) << 24) |
    (parseInt(b,10) << 16) |
    (parseInt(c,10) <<  8) |
     parseInt(d,10);
  const mask = prefix === 0 ? 0 : (~0 << (32 - prefix)) >>> 0;
  const net = ipNum & mask;
  const size = 2 ** (32 - prefix);
  const start = prefix <= 30 ? 1 : 0;
  const end = prefix <= 30 ? size - 2 : size - 1;

  const ips: string[] = [];
  for (let i = start; i <= end; i++) {
    const val = (net + i) >>> 0;
    const A = (val >>> 24) & 0xff;
    const B = (val >>> 16) & 0xff;
    const C = (val >>> 8) & 0xff;
    const D = val & 0xff;
    ips.push(`${A}.${B}.${C}.${D}`);
  }
  return ips;
}

function estimateHosts(cidr: string): number {
  const m = cidr.trim().match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)\/(\d{1,2})$/);
  if (!m) return 0;
  const prefix = parseInt(m[5], 10);
  if (prefix < 0 || prefix > 32) return 0;
  const size = 2 ** (32 - prefix);
  const usable = prefix <= 30 ? Math.max(0, size - 2) : size;
  return usable;
}

// Quick UI helpers
function resetResult() {
  progress.value = [];
  report.value = null;
  err.value = null;
}
function fmtMs(v?: number | null) {
  return v == null ? "-" : `${v} ms`;
}

const targetCount = computed(() =>
  form.mode === "cidr" ? estimateHosts(form.cidr) : new Set(
    (form.list || "").split(/[\s,;]+/).map(s => s.trim()).filter(Boolean)
  ).size
);

const canStart = computed(() =>
  targetCount.value > 0 &&
  (form.mode !== "cidr" || targetCount.value <= MAX_EXPAND) &&
  !loading.value && !running.value
);

const sent = computed(() =>
  progress.value.length > 0 ? progress.value[progress.value.length - 1].done : 0
);
const total = computed(() =>
  progress.value.length > 0 ? progress.value[progress.value.length - 1].total : (report.value?.total ?? 0)
);
const pct = computed(() =>
  total.value > 0 ? Math.min(100, Math.round((sent.value / total.value) * 100)) : 0
);

function parseTargetsForStart(): string[] {
  if (form.mode === "cidr") {
    return expandIpv4Cidr(form.cidr, MAX_EXPAND);
  }
  const raw = form.list || "";
  const tokens = raw.split(/[\s,;]+/).map(s => s.trim()).filter(Boolean);
  return Array.from(new Set(tokens));
}


let unlistenProgress: UnlistenFn | null = null;
let unlistenDone: UnlistenFn | null = null;

async function startScan() {
  resetResult();

  const targets = parseTargetsForStart();
  if (targets.length === 0) {
    const est = form.mode === "cidr" ? estimateHosts(form.cidr) : 0;
    err.value = est > MAX_EXPAND
      ? `Target too large (${est} hosts). Please use a narrower CIDR or increase the limit.`
      : "No targets. Add CIDR or IP list.";
    return;
  }

  running.value = true;
  loading.value = true;
  const setting: HostScanSetting = {
    targets,
    hop_limit: form.hop_limit,
    timeout_ms: form.timeout_ms,
    count: form.count,
    payload: form.payload || null,
    ordered: form.ordered,
    concurrency: form.concurrency || null,
  };

  try {
    const rep = await invoke<HostScanReport>("host_scan", { setting });
    report.value = rep;
  } catch (e: any) {
    err.value = String(e?.message ?? e);
  } finally {
    loading.value = false;
    running.value = false;
  }
}

onMounted(async () => {
  // progress
  unlistenProgress = await listen<HostScanProgress>("hostscan:progress", ev => {
    const p = ev.payload;
    if (!p) return;
    progress.value = [...progress.value, p];
  });
  // done
  unlistenDone = await listen<HostScanReport>("hostscan:done", ev => {
    if (ev.payload) report.value = ev.payload;
    running.value = false;
  });
});

onBeforeUnmount(() => {
  unlistenProgress?.();
  unlistenDone?.();
});
</script>

<template>
  <div ref="wrapRef" class="p-3 lg:p-4 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-start gap-3">
      <div class="flex items-center gap-3 min-w-0 flex-wrap">
        <!-- Mode -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Mode</label>
          <Select
            v-model="form.mode"
            :options="[
              { label: 'CIDR (IPv4)', value: 'cidr' },
              { label: 'List (IPs)',  value: 'list' },
            ]"
            optionLabel="label"
            optionValue="value"
            class="min-w-40"
          />
        </div>

        <!-- CIDR / List -->
        <div v-if="form.mode==='cidr'" class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">CIDR</label>
          <InputText v-model="form.cidr" placeholder="e.g. 192.168.1.0/24" class="w-[220px]" />
        </div>
        <div v-else class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">IP List (newline / space / comma)</label>
          <Textarea v-model="form.list" rows="2" class="w-[280px]" />
        </div>

        <!-- Options -->
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">Timeout (ms)</label>
          <InputNumber v-model="form.timeout_ms" :min="100" :max="60000" :step="100" inputClass="w-[120px]" />
        </div>
        <div class="flex flex-col gap-1">
          <label class="text-xs text-surface-500">TTL / Hop Limit</label>
          <InputNumber v-model="form.hop_limit" :min="1" :max="255" inputClass="w-[120px]" />
        </div>
        <div class="flex items-center gap-2 mt-4">
          <Checkbox v-model="form.ordered" :binary="true" inputId="ordered" />
          <label for="ordered" class="text-sm">Ordered</label>
        </div>

        <!-- Target count preview -->
        <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-2 mt-4">
          <div class="text-surface-500 text-xs">Targets</div>
          <div class="font-medium">{{ targetCount }}</div>
        </div>
      </div>

      <div class="flex flex-wrap items-end gap-3 justify-end">
        <!-- Actions -->
        <div class="flex items-center gap-2 mt-4">
          <Button label="Start" icon="pi pi-play" :disabled="!canStart" :loading="loading" @click="startScan" />
        </div>
      </div>
    </div>

    <div class="flex-1 min-h-0">
    <!-- Scrollable content -->
    <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-3">
        <!-- Progress -->
        <Card>
          <template #title>Progress</template>
          <template #content>
            <div class="flex items-center justify-between mb-2">
              <div class="text-sm text-surface-500">Scanned: {{ sent }} / {{ total }}</div>
              <div class="text-sm text-surface-500">{{ pct }}%</div>
            </div>
            <ProgressBar :value="pct" />

            <div class="mt-3">
              <DataTable
                :value="progress"
                size="small"
                stripedRows
                class="text-sm"
                :rows="10" paginator :rowsPerPageOptions="[10,20,50]"
                sortMode="single" sortField="ip_addr" :sortOrder="1"
              >
                <Column field="ip_addr" header="IP" sortable />
                <Column header="State" style="width: 120px" sortable>
                  <template #body="{ data }">
                    <Tag :value="data.state" :severity="data.state==='Alive' ? 'success' : 'warn'" />
                  </template>
                </Column>
                <Column header="RTT" sortable>
                  <template #body="{ data }">{{ fmtMs(data.rtt_ms) }}</template>
                </Column>
                <Column header="Message">
                  <template #body="{ data }">
                    <span class="text-surface-500" v-if="data.message">{{ data.message }}</span>
                    <span v-else>-</span>
                  </template>
                </Column>
              </DataTable>
            </div>
          </template>
        </Card>

        <!-- Summary -->
        <Card>
          <template #title>Summary</template>
          <template #content>
            <div v-if="err" class="text-red-500 text-sm mb-2">{{ err }}</div>
            <template v-if="report">
              <div class="grid grid-cols-2 gap-3 text-sm">
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Alive</div>
                  <div class="font-medium">{{ report.alive.length }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                  <div class="text-surface-500 text-xs">Unreachable</div>
                  <div class="font-medium">{{ report.unreachable.length }}</div>
                </div>
              </div>

              <div class="mt-3">
                <div class="font-semibold mb-1">Alive Hosts</div>
                <DataTable
                  :value="report.alive.map(([ip, rtt]) => ({ ip, rtt }))"
                  size="small" stripedRows class="text-sm"
                  :rows="10" paginator :rowsPerPageOptions="[10,20,50]"
                  sortMode="single" sortField="ip" :sortOrder="1"
                >
                  <Column field="ip" header="IP" sortable />
                  <Column header="RTT" sortable>
                    <template #body="{ data }">{{ fmtMs(data.rtt) }}</template>
                  </Column>
                </DataTable>
              </div>
            </template>
            <template v-else>
              <div class="text-surface-500 text-sm">Run a scan to see results.</div>
            </template>
          </template>
        </Card>
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>
