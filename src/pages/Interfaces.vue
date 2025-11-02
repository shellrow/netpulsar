<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { NetworkInterface } from "../types/net"; 
import { ipListToString, formatBps, formatBytesPerSec, formatBytes } from "../types/net"; 
import { DataTableRowSelectEvent } from 'primevue/datatable';
import { fmtIfType, fmtDate, hexFlags, severityByOper} from "../utils/formatter";

const wrapRef = ref<HTMLElement|null>(null);
const toolbarRef = ref<HTMLElement|null>(null);
const tableHeight = ref("400px");  

type UnitPref = "bytes" | "bits";
const LS_BPS_UNIT = "netpulsar:set:bps_unit";
const bpsUnit = ref<UnitPref>((localStorage.getItem(LS_BPS_UNIT) as UnitPref) || "bytes");

function refreshUnitPref() {
  const v = (localStorage.getItem(LS_BPS_UNIT) as UnitPref) || "bytes";
  bpsUnit.value = v === "bits" ? "bits" : "bytes";
}

let ro: ResizeObserver | null = null;
let rafId: number | null = null;
let scheduled = false;

function _calcTableHeight(): string {
  const wrap = wrapRef.value;
  if (!wrap) return tableHeight.value;

  const cs = getComputedStyle(wrap);
  const padY = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  const inner = wrap.clientHeight - padY;

  const toolbarH = toolbarRef.value?.offsetHeight ?? 0;
  const EXTRA_OFFSET = 80;
  const GAP = 12;
  const px = Math.max(200, inner - toolbarH - EXTRA_OFFSET - GAP);
  return `${Math.floor(px)}px`;
}

function scheduleRecalc() {
  if (scheduled) return;
  scheduled = true;
  rafId && cancelAnimationFrame(rafId);
  rafId = requestAnimationFrame(() => {
    scheduled = false;
    const next = _calcTableHeight();
    if (next !== tableHeight.value) {
      tableHeight.value = next;
    }
  });
}

const rxHeader = computed(() => (bpsUnit.value === "bits" ? "RX bps" : "RX B/s"));
const txHeader = computed(() => (bpsUnit.value === "bits" ? "TX bps" : "TX B/s"));

function fmtThroughput(v?: number): string {
  const n = v ?? 0;
  return bpsUnit.value === "bits" ? formatBps(n * 8) : formatBytesPerSec(n);
}

const loading = ref(false);
const rows = ref<NetworkInterface[]>([]);
const selectedInterface = ref<NetworkInterface | null>(null);
const dialogVisible = ref(false);
const globalFilter = ref("");
const visibleColumns = ref<string[]>([
  "name",
  "oper",
  "ipv4",
  "ipv6",
  "rx_bytes_per_sec",
  "tx_bytes_per_sec",
  "mtu",
  "mac",
]);

async function fetchInterfaces() {
  //loading.value = true;
  try {
    const data = (await invoke("get_network_interfaces")) as NetworkInterface[];
    rows.value = data;
  } finally {
    //loading.value = false;
  }
}

async function reloadInterfaces() {
  loading.value = true;
  try {
    await invoke("reload_interfaces");
    await fetchInterfaces();
  } finally {
    loading.value = false;
  }
}

let debouncing = false;
let unlistenStats: UnlistenFn | null = null;
let unlistenIfaces: UnlistenFn | null = null;

async function onStatsUpdated() {
  if (debouncing) return;
  debouncing = true;
  setTimeout(async () => {
    refreshUnitPref();
    await fetchInterfaces();
    debouncing = false;
  }, 500);
}

async function onInterfacesUpdated() {
    loading.value = true;
    try {
      refreshUnitPref();
      await fetchInterfaces();
    } finally {
      loading.value = false;
    }
}

const onRowSelect = (event: DataTableRowSelectEvent) => {
    const iface: NetworkInterface = event.data;
    selectedInterface.value = iface;
    dialogVisible.value = true;
};

const onRowUnselect = (_event: DataTableRowSelectEvent) => {
    dialogVisible.value = false;
}

onMounted(async () => {
  await fetchInterfaces();
  refreshUnitPref();
  unlistenStats = await listen("stats_updated", onStatsUpdated);
  unlistenIfaces = await listen("interfaces_updated", onInterfacesUpdated);

  await nextTick();
  tableHeight.value = _calcTableHeight();
  ro = new ResizeObserver(() => {
    scheduleRecalc();
  });
  if (wrapRef.value) ro.observe(wrapRef.value);
  if (toolbarRef.value) ro.observe(toolbarRef.value);
  window.addEventListener("resize", scheduleRecalc);
  window.addEventListener("storage", refreshUnitPref);
});

onBeforeUnmount(() => {
  unlistenStats?.();
  unlistenIfaces?.();

  ro?.disconnect();
  if (rafId) cancelAnimationFrame(rafId);
  window.removeEventListener("resize", scheduleRecalc);
  window.removeEventListener("storage", refreshUnitPref);
});

const filtered = computed(() => {
  const q = globalFilter.value.trim().toLowerCase();
  if (!q) return rows.value;
  return rows.value.filter(r => {
    const hay =
      [
        r.name,
        r.friendly_name ?? "",
        r.oper_state ?? "",
        ipListToString(r.ipv4),
        ipListToString(r.ipv6),
        r.mac_addr ?? "",
        String(r.mtu ?? ""),
      ]
        .join(" ")
        .toLowerCase();
    return hay.includes(q);
  });
});
</script>

<template>
  <div ref="wrapRef" class="p-3 lg:p-4 flex flex-col gap-3 flex-1 min-h-0 h-full">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2 w-full shrink-0">
        <!-- Left -->
        <div class="flex items-center gap-2 min-w-0">
            <span class="text-surface-500 dark:text-surface-400 text-sm"> Network Interfaces
            ({{ filtered.length }})
            </span>
        </div>
        <!-- Right -->
        <div class="flex flex-wrap items-center gap-2 justify-end">
            <!-- MultiSelect -->
            <MultiSelect
            v-model="visibleColumns"
            :options="[
                { label: 'Name', value: 'name' },
                { label: 'State', value: 'oper' },
                { label: 'IPv4', value: 'ipv4' },
                { label: 'IPv6', value: 'ipv6' },
                { label: 'RX Bps', value: 'rx_bytes_per_sec' },
                { label: 'TX Bps', value: 'tx_bytes_per_sec' },
                { label: 'MTU', value: 'mtu' },
                { label: 'MAC', value: 'mac' },
            ]"
            optionLabel="label"
            optionValue="value"
            placeholder="Columns"
            class="min-w-40"
            display="chip" 
            :maxSelectedLabels="4" 
            />
            <!-- Search -->
            <InputGroup class="max-w-[220px]">
            <InputGroupAddon><i class="pi pi-search"></i></InputGroupAddon>
            <InputText
                v-model="globalFilter"
                placeholder="Search (name/ip/mac...)"
                class="flex-1 min-w-0"
            />
            </InputGroup>
            <!-- Refresh -->
            <Button
            outlined
            icon="pi pi-refresh"
            severity="secondary"
            class="w-9 h-9"
            :loading="loading"
            @click="reloadInterfaces"
            />
        </div>
    </div>
    <!-- Table -->
    <DataTable
      :value="filtered"
      :loading="loading"
      dataKey="index"
      paginator
      :rows="15"
      :rowsPerPageOptions="[15, 30, 50, 100]"
      sortMode="multiple"
      scrollable
      :scrollHeight="tableHeight"
      class="text-sm"
      stripedRows
      selectionMode="single"
      @rowSelect="onRowSelect"
      @rowUnselect="onRowUnselect"
    >
      <!-- Name -->
      <Column v-if="visibleColumns.includes('name')" field="display_name" header="Name" sortable />
      <!-- State -->
      <Column v-if="visibleColumns.includes('oper')" header="State" :sortable="true">
        <template #body="{ data }">
          <Tag
            :value="data.oper_state ?? '-'"
            :severity="(data.oper_state ?? '').toLowerCase() === 'up' ? 'success'
                      : (data.oper_state ?? '').toLowerCase() === 'down' ? 'danger' : 'secondary'"
          />
        </template>
      </Column>
      <!-- MAC -->
      <Column v-if="visibleColumns.includes('mac')" field="mac" header="MAC" sortable style="min-width: 150px">
        <template #body="{ data }">{{ data.mac_addr ?? '-' }}</template>
      </Column>
      <!-- IPv4 -->
      <Column v-if="visibleColumns.includes('ipv4')" header="IPv4" :sortable="false" style="min-width: 220px">
        <template #body="{ data }">
          {{ ipListToString(data.ipv4) || '-' }}
        </template>
      </Column>
      <!-- IPv6 -->
      <Column v-if="visibleColumns.includes('ipv6')" header="IPv6" :sortable="false" style="min-width: 220px">
        <template #body="{ data }">
          {{ ipListToString(data.ipv6) || '-' }}
        </template>
      </Column>
      <!-- RX Bps -->
      <Column v-if="visibleColumns.includes('rx_bytes_per_sec')" :header="rxHeader" sortField="stats.rx_bytes_per_sec" :sortable="true" style="min-width: 120px">
        <template #body="{ data }">
          {{ fmtThroughput(data.stats.rx_bytes_per_sec) }}
        </template>
      </Column>
      <!-- TX Bps -->
      <Column v-if="visibleColumns.includes('tx_bytes_per_sec')" :header="txHeader" sortField="stats.tx_bytes_per_sec" :sortable="true" style="min-width: 120px">
        <template #body="{ data }">
          {{ fmtThroughput(data.stats.tx_bytes_per_sec) }}
        </template>
      </Column>
      <!-- MTU -->
      <Column v-if="visibleColumns.includes('mtu')" field="mtu" header="MTU" sortable style="width: 96px">
        <template #body="{ data }">{{ data.mtu ?? '-' }}</template>
      </Column>
    </DataTable>
  </div>

    <Dialog
        v-model:visible="dialogVisible"
        modal
        appendTo="body"
        :draggable="false"
        :style="{ width: '52rem', maxWidth: '95vw' }"
        >
        <!-- Header -->
        <template #header>
            <div class="flex items-center gap-3 min-w-0">
            <i class="pi pi-arrows-h text-surface-500"></i>
            <span class="font-bold truncate">{{ selectedInterface?.name ?? 'Interface' }}</span>
            <Tag
                v-if="selectedInterface?.oper_state"
                :value="selectedInterface?.oper_state"
                :severity="severityByOper(selectedInterface?.oper_state)"
                class="ml-2"
            />
            <Tag
                v-if="selectedInterface?.default"
                value="Default"
                severity="info"
                class="ml-1"
            />
            </div>
        </template>
        <!-- Body -->
        <div class="flex flex-col gap-4 text-sm">
            <!-- Overview/Performance -->
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <!-- Overview -->
            <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
                <div class="font-semibold mb-2">Overview</div>
                <div class="space-y-1">
                <div><span class="text-surface-500">Index:</span> <span class="font-mono">{{ selectedInterface?.index }}</span></div>
                <div><span class="text-surface-500">Type:</span> <span>{{ fmtIfType(selectedInterface?.if_type) }}</span></div>
                <div><span class="text-surface-500">Friendly:</span> <span>{{ selectedInterface?.friendly_name ?? '-' }}</span></div>
                <div><span class="text-surface-500">Description:</span> <span>{{ selectedInterface?.description ?? '-' }}</span></div>
                <div><span class="text-surface-500">MAC:</span> <span class="font-mono">{{ selectedInterface?.mac_addr ?? '-' }}</span></div>
                <div><span class="text-surface-500">MTU:</span> <span>{{ selectedInterface?.mtu ?? '-' }}</span></div>
                <div><span class="text-surface-500">Flags:</span> <span class="font-mono">{{ hexFlags(selectedInterface?.flags) }}</span></div>
                </div>
            </div>
            <!-- Performance -->
            <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
                <div class="font-semibold mb-2">Performance</div>
                <div class="grid grid-cols-2 gap-3">
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">{{ rxHeader }}</div>
                    <div class="text-base font-semibold">{{ fmtThroughput(selectedInterface?.stats?.rx_bytes_per_sec || 0) }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">{{ txHeader }}</div>
                    <div class="text-base font-semibold">{{ fmtThroughput(selectedInterface?.stats?.tx_bytes_per_sec || 0) }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">RX total</div>
                    <div class="font-mono">{{ formatBytes(selectedInterface?.stats?.rx_bytes || 0) }}</div>
                </div>
                <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">TX total</div>
                    <div class="font-mono">{{ formatBytes(selectedInterface?.stats?.tx_bytes || 0) }}</div>
                </div>
                </div>
                <div class="text-xs text-surface-500 mt-2">
                Snapshot: {{ selectedInterface?.stats?.timestamp ? fmtDate(selectedInterface?.stats?.timestamp) : '-' }}
                </div>
                <div class="text-xs text-surface-500 mt-1">
                Link Speed:
                <span v-if="selectedInterface?.receive_speed">RX {{ formatBps(selectedInterface?.receive_speed) }}</span>
                <span v-else>RX -</span>
                /
                <span v-if="selectedInterface?.transmit_speed">TX {{ formatBps(selectedInterface?.transmit_speed) }}</span>
                <span v-else>TX -</span>
                </div>
            </div>
            </div>
            <!-- IP Addresses -->
            <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
            <div class="font-semibold mb-2">IP Addresses</div>
            <div class="mb-2">
                <span class="text-surface-500 text-xs">IPv4</span>
                <div class="mt-1 flex flex-wrap gap-2">
                <Chip v-for="(v,i) in (selectedInterface?.ipv4 ?? [])" :key="'v4-'+i" :label="typeof v==='string' ? v : `${v.addr}/${v.prefix_len}`" class="font-mono" />
                <span v-if="(selectedInterface?.ipv4?.length ?? 0) === 0">-</span>
                </div>
            </div>
            <div>
                <span class="text-surface-500 text-xs">IPv6</span>
                <div class="mt-1 flex flex-wrap gap-2">
                <Chip v-for="(v,i) in (selectedInterface?.ipv6 ?? [])" :key="'v6-'+i" :label="typeof v==='string' ? v : `${v.addr}/${v.prefix_len}`" class="font-mono" />
                <span v-if="(selectedInterface?.ipv6?.length ?? 0) === 0">-</span>
                </div>
                <div class="text-xs text-surface-500 mt-2" v-if="(selectedInterface?.ipv6_scope_ids?.length ?? 0) > 0">
                Scope IDs: <span class="font-mono">{{ (selectedInterface?.ipv6_scope_ids ?? []).join(', ') }}</span>
                </div>
            </div>
            </div>
            <!-- Routing / DNS -->
            <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
            <div class="font-semibold mb-2">Routing / DNS</div>
            <div class="flex flex-wrap gap-4">
                <div>
                    <div class="text-surface-500 text-xs">Gateway</div>
                    <div class="mt-1">
                        <div v-if="selectedInterface?.gateway">
                        <div class="font-mono">MAC: {{ selectedInterface.gateway.mac_addr }}</div>
                        <div v-if="selectedInterface.gateway.ipv4.length > 0">
                            IPv4: <span class="font-mono">{{ selectedInterface.gateway.ipv4.join(', ') }}</span>
                        </div>
                        <div v-if="selectedInterface.gateway.ipv6.length > 0">
                            IPv6: <span class="font-mono">{{ selectedInterface.gateway.ipv6.join(', ') }}</span>
                        </div>
                        </div>
                        <span v-else>-</span>
                    </div>
                </div>
                <div>
                <div class="text-surface-500 text-xs">DNS</div>
                <div class="mt-1 flex flex-wrap gap-2">
                    <Chip v-for="(d,i) in (selectedInterface?.dns_servers ?? [])" :key="'dns-'+i" :label="d" class="font-mono" />
                    <span v-if="(selectedInterface?.dns_servers?.length ?? 0) === 0">-</span>
                </div>
                </div>
            </div>
            </div>
        </div>
        <!-- Footer -->
        <template #footer>
            <Button label="Close" text severity="secondary" @click="dialogVisible = false" />
        </template>
    </Dialog>
</template>
