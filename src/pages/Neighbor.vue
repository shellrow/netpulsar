<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { HostScanProgress, NeighborScanReport } from "../types/probe";
import { Ipv4Net } from "../types/net";

const running = ref(false);
const loading = ref(false);
const err = ref<string | null>(null);

const progress = ref<HostScanProgress[]>([]);
const report = ref<NeighborScanReport | null>(null);

const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight({ extra: 28, gap: 12, min: 220 });

const netMap = ref<Record<string, Ipv4Net>>({});
const selectedIf = ref<string | null>(null);

const ifOptions = computed(() =>
  Object.entries(netMap.value).map(([name, net]) => {
    let cidr = "-";
    if (typeof net === "string") {
      cidr = net;
    } else if (net && typeof net === "object") {
      cidr = `${net.addr}/${net.prefix_len}`;
    }
    return { label: `${name}  ${cidr}`, value: name };
  })
);

const selectedCidr = computed(() => {
  const name = selectedIf.value;
  if (!name) return "-";
  const net = netMap.value[name];
  if (!net) return "-";
  return typeof net === "string" ? net : `${net.addr}/${net.prefix_len}`;
});

function resetAll() {
  err.value = null;
  report.value = null;
  progress.value = [];
}

function fmtMs(v?: number | null) {
  return v == null ? "-" : `${v} ms`;
}

const sent = computed(() => (progress.value.length > 0 ? progress.value[progress.value.length - 1].done : 0));
const total = computed(() =>
  progress.value.length > 0 ? progress.value[progress.value.length - 1].total : report.value?.total ?? 0
);
const pct = computed(() => (total.value > 0 ? Math.min(100, Math.round((sent.value / total.value) * 100)) : 0));

let unlistenHostProgress: UnlistenFn | null = null;
let unlistenNeighborStart: UnlistenFn | null = null;
let unlistenNeighborDone: UnlistenFn | null = null;

async function fetchNetworkAddressMap() {
  try {
    const m = await invoke<Record<string, Ipv4Net>>("get_network_address_map");
    netMap.value = m ?? {};
    const first = Object.keys(netMap.value)[0];
    selectedIf.value = first ?? null;
  } catch (e: any) {
    err.value = `failed to load networks: ${String(e?.message ?? e)}`;
  }
}

async function startScan() {
  resetAll();
  running.value = true;
  loading.value = true;
  try {
    const rep = await invoke<NeighborScanReport>("neighbor_scan", {
      ifaceName: selectedIf.value ?? null,
    });
    report.value = rep;
  } catch (e: any) {
    err.value = String(e?.message ?? e);
  } finally {
    loading.value = false;
    running.value = false;
  }
}

onMounted(async () => {
  await nextTick();

  unlistenHostProgress = await listen<HostScanProgress>("hostscan:progress", ev => {
    const p = ev.payload;
    if (!p) return;
    progress.value = [...progress.value, p];
  });

  unlistenNeighborStart = await listen<string>("neighborscan:start", () => {
    running.value = true;
    err.value = null;
  });

  unlistenNeighborDone = await listen<string>("neighborscan:done", () => {
    running.value = false;
  });

  await fetchNetworkAddressMap();
});

onBeforeUnmount(() => {
  unlistenHostProgress?.();
  unlistenNeighborStart?.();
  unlistenNeighborDone?.();
});
</script>

<template>
  <div ref="wrapRef" class="p-3 lg:p-4 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-start gap-3">
      <div class="flex items-center gap-3 min-w-0 flex-wrap">
        <div class="text-surface-500 dark:text-surface-400 text-sm">Neighbor Scan</div>

        <Select
          v-model="selectedIf"
          :options="ifOptions"
          optionLabel="label"
          optionValue="value"
          placeholder="Select interface"
          class="min-w-64"
          :disabled="running || loading"
        />
        <Chip v-if="selectedIf" :label="selectedCidr" class="font-mono" />
        <span v-else class="text-surface-500 text-xs">No eligible interface</span>
      </div>

      <div class="flex items-center justify-end gap-2">
        <Button
          label="Start"
          icon="pi pi-play"
          :disabled="running || !selectedIf"
          :loading="loading"
          @click="startScan"
        />
      </div>
    </div>

    <!-- Scrollable content -->
    <div class="flex-1 min-h-0">
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
                  :rows="10"
                  paginator
                  :rowsPerPageOptions="[10,20,50]"
                  sortMode="single"
                  sortField="ip_addr"
                  :sortOrder="1"
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

          <!-- Result -->
          <Card>
            <template #title>Neighbors</template>
            <template #content>
              <div v-if="err" class="text-red-500 text-sm mb-2">{{ err }}</div>

              <template v-if="report">
                <div class="grid grid-cols-2 gap-3 text-sm mb-3">
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">Total Alive</div>
                    <div class="font-medium">{{ report.neighbors.length }}</div>
                  </div>
                  <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                    <div class="text-surface-500 text-xs">Scanned</div>
                    <div class="font-medium">{{ report.total }}</div>
                  </div>
                </div>

                <DataTable
                  :value="report.neighbors"
                  size="small"
                  stripedRows
                  class="text-sm"
                  :rows="10"
                  paginator
                  :rowsPerPageOptions="[10,20,50]"
                  sortMode="multiple"
                >
                  <Column field="ip_addr" header="IP" sortable />
                  <Column field="mac_addr" header="MAC" sortable>
                    <template #body="{ data }">
                      <span class="font-mono">{{ data.mac_addr ?? '-' }}</span>
                    </template>
                  </Column>
                  <Column field="vendor" header="Vendor" sortable>
                    <template #body="{ data }">
                      <span>{{ data.vendor ?? '-' }}</span>
                    </template>
                  </Column>
                  <Column field="rtt_ms" header="RTT" sortable>
                    <template #body="{ data }">{{ fmtMs(data.rtt_ms) }}</template>
                  </Column>
                  <Column field="tags" header="Tags" sortable>
                    <template #body="{ data }">
                      <div class="flex flex-wrap gap-1">
                        <Tag
                          v-for="t in (data.tags || [])"
                          :key="t"
                          :value="t"
                          :severity="t==='Gateway' ? 'warn' : (t==='Self' ? 'info' : (t==='DNS' ? 'secondary' : 'contrast'))"
                          class="text-xs"
                        />
                        <span v-if="!data.tags || data.tags.length===0" class="text-surface-500">-</span>
                      </div>
                    </template>
                  </Column>
                </DataTable>
              </template>

              <template v-else>
                <div class="text-surface-500 text-sm">Press Start to run a neighbor scan.</div>
              </template>
            </template>
          </Card>
        </div>
      </ScrollPanel>
    </div>
  </div>
</template>
