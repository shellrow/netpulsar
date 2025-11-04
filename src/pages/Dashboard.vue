<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { NetworkInterface } from "../types/net";
import { formatBps, formatBytesPerSec, formatBytes } from "../types/net";
import type { IpInfoDual, IpInfo } from "../types/internet";
import type { SysInfo } from "../types/system";
import { nv, fmtIfType, hexFlags, severityByOper } from "../utils/formatter";
import { useScrollPanelHeight } from "../composables/useScrollPanelHeight";
import { usePrivacyGate } from "../composables/usePrivacyGate";

const { wrapRef, toolbarRef, panelHeight } = useScrollPanelHeight();
const { publicIpVisible, togglePublicIp, pubIpGate, hostnameVisible, toggleHostname } = usePrivacyGate();

const loading = ref(false);
const prevDefaultIfaceName = ref<string | null>(null);
const ifaces = ref<NetworkInterface[]>([]);
const ipDual = ref<IpInfoDual | null>(null);
const sys = ref<SysInfo | null>(null);

type UnitPref = "bytes" | "bits";
const LS_BPS_UNIT = "netpulsar:set:bps_unit";
const bpsUnit = ref<UnitPref>((localStorage.getItem(LS_BPS_UNIT) as UnitPref) || "bytes");

function refreshUnitPref() {
  const v = (localStorage.getItem(LS_BPS_UNIT) as UnitPref) || "bytes";
  bpsUnit.value = v === "bits" ? "bits" : "bytes";
}
const rxLabel = computed(() => (bpsUnit.value === "bits" ? "RX bps" : "RX B/s"));
const txLabel = computed(() => (bpsUnit.value === "bits" ? "TX bps" : "TX B/s"));
function formatThroughput(v?: number): string {
  const n = v ?? 0;
  return bpsUnit.value === "bits" ? formatBps(n * 8) : formatBytesPerSec(n);
}

const defaultIface = computed<NetworkInterface | null>(() => {
  const d = ifaces.value.find(i => i.default) ?? null;
  if (d) return d;
  const cand = ifaces.value.find(i => (i.oper_state ?? "").toLowerCase() === "up" && !!i.gateway);
  return cand ?? null;
});
const v4 = computed<IpInfo | null>(() => ipDual.value?.ipv4 ?? null);
const v6 = computed<IpInfo | null>(() => ipDual.value?.ipv6 ?? null);

async function fetchInterfaces() {
  try {
    const data = (await invoke("get_network_interfaces")) as NetworkInterface[];
    ifaces.value = data;
  } finally {
    /* noop */
  }
}
async function fetchPublicIp() {
  try {
    const data = (await invoke("get_public_ip_info")) as IpInfoDual;
    ipDual.value = data;
  } finally {
    /* noop */
  }
}
async function fetchSysInfo() {
  try {
    const data = (await invoke("get_sys_info")) as SysInfo;
    sys.value = data;
  } finally {
    /* noop */
  }
}
async function fetchAll() {
  loading.value = true;
  try {
    const [ifs, ip, si] = await Promise.all([
      invoke("get_network_interfaces") as Promise<NetworkInterface[]>,
      invoke("get_public_ip_info") as Promise<IpInfoDual>,
      invoke("get_sys_info") as Promise<SysInfo>,
    ]);
    ifaces.value = ifs ?? [];
    ipDual.value = ip ?? null;
    sys.value = si ?? null;

    const d = defaultIface.value;
    if (d) prevDefaultIfaceName.value = d.name;
  } finally {
    loading.value = false;
  }
}

let unlistenStats: UnlistenFn | null = null;
let unlistenIfaces: UnlistenFn | null = null;
let debouncing = false;

async function onStatsUpdated() {
  // Debounce to avoid excessive refreshes when stats are frequent
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
    await fetchSysInfo();
    // If default interface changed, refetch public IP info
    if (defaultIface.value) {
      if (defaultIface.value.name !== prevDefaultIfaceName.value) {
        await fetchPublicIp();
        prevDefaultIfaceName.value = defaultIface.value.name;
      }
    }
  } finally {
    loading.value = false;
  }
}

function togglePrivacy() {
  togglePublicIp();
  toggleHostname();
}

onMounted(async () => {
  refreshUnitPref();
  await fetchAll();
  unlistenStats = await listen("stats_updated", onStatsUpdated);
  unlistenIfaces = await listen("interfaces_updated", onInterfacesUpdated);
  window.addEventListener("storage", refreshUnitPref);
});
onBeforeUnmount(() => {
  unlistenStats?.();
  unlistenIfaces?.();
  window.removeEventListener("storage", refreshUnitPref);
});
</script>

<template>
  <div ref="wrapRef" class="px-3 pt-3 pb-1 lg:px-4 lg:pt-4 lg:pb-2 flex flex-col gap-2 h-full min-h-0">
    <!-- Toolbar -->
    <div ref="toolbarRef" class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">Overview</span>
        <div v-if="!sys" class="text-surface-500">Loading...</div>
        <div v-else class="text-surface-500 text-sm">
          <div v-if="hostnameVisible">
            <span class="text-surface-500 dark:text-surface-400 text-sm mr-3">{{ sys.hostname }}</span>
          </div>
        </div>
      </div>
      <div class="flex items-center gap-2 justify-end">
        <Button outlined :icon="publicIpVisible ? 'pi pi-eye' : 'pi pi-eye-slash'" @click="togglePrivacy" class="w-9 h-9" severity="secondary" />
        <Button outlined icon="pi pi-refresh" :loading="loading" @click="fetchAll" class="w-9 h-9" severity="secondary" />
      </div>
    </div>

    <div class="flex-1 min-h-0">
    <!-- Scrollable content -->
    <ScrollPanel :style="{ width: '100%', height: panelHeight }" class="flex-1 min-h-0">
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-2 content-start auto-rows-max p-1">
        <!-- Default Interface -->
        <Card>
          <template #title>Default Interface</template>
          <template #content>
            <div class="flex flex-col gap-4 text-sm">
              <div v-if="!defaultIface" class="text-surface-500">No default interface detected.</div>

              <div v-else class="flex flex-col gap-4 text-sm">
                <!-- Header -->
                <div class="flex items-center gap-3">
                  <i class="pi pi-arrows-h text-surface-500"></i>
                  <span class="font-bold truncate">{{ defaultIface.name }}</span>
                  <Tag
                    v-if="defaultIface.oper_state"
                    :value="defaultIface.oper_state"
                    :severity="severityByOper(defaultIface.oper_state)"
                  />
                  <Tag v-if="defaultIface.default" value="Default" severity="info" />
                </div>

                <!-- Overview / Performance -->
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <!-- Overview -->
                  <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
                    <div class="font-semibold mb-2">Overview</div>
                    <div class="space-y-1">
                      <div><span class="text-surface-500">Index:</span> <span class="font-mono">{{ defaultIface.index }}</span></div>
                      <div><span class="text-surface-500">Type:</span> <span>{{ fmtIfType(defaultIface.if_type) }}</span></div>
                      <div><span class="text-surface-500">Friendly:</span> <span>{{ defaultIface.friendly_name ?? '-' }}</span></div>
                      <div><span class="text-surface-500">Description:</span> <span>{{ defaultIface.description ?? '-' }}</span></div>
                      <div><span class="text-surface-500">MAC:</span> <span class="font-mono">{{ defaultIface.mac_addr ?? '-' }}</span></div>
                      <div><span class="text-surface-500">MTU:</span> <span>{{ defaultIface.mtu ?? '-' }}</span></div>
                      <div><span class="text-surface-500">Flags:</span> <span class="font-mono">{{ hexFlags(defaultIface.flags) }}</span></div>
                    </div>
                  </div>

                  <!-- Performance -->
                  <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
                    <div class="font-semibold mb-2">Performance</div>
                    <div class="grid grid-cols-2 gap-3">
                      <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                        <div class="text-surface-500 text-xs">{{ rxLabel }}</div>
                        <div class="text-base font-semibold">{{ formatThroughput(defaultIface.stats?.rx_bytes_per_sec || 0) }}</div>
                      </div>
                      <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                        <div class="text-surface-500 text-xs">{{ txLabel }}</div>
                        <div class="text-base font-semibold">{{ formatThroughput(defaultIface.stats?.tx_bytes_per_sec || 0) }}</div>
                      </div>
                      <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                        <div class="text-surface-500 text-xs">RX total bytes</div>
                        <div class="font-mono">{{ formatBytes(defaultIface.stats?.rx_bytes || 0) }}</div>
                      </div>
                      <div class="rounded-lg bg-surface-50 dark:bg-surface-900 p-3">
                        <div class="text-surface-500 text-xs">TX total bytes</div>
                        <div class="font-mono">{{ formatBytes(defaultIface.stats?.tx_bytes || 0) }}</div>
                      </div>
                    </div>
                    <div class="text-xs text-surface-500 mt-1">
                      Link Speed:
                      <span v-if="defaultIface.receive_speed">RX {{ formatBps(defaultIface.receive_speed) }}</span>
                      <span v-else>RX -</span>
                      /
                      <span v-if="defaultIface.transmit_speed">TX {{ formatBps(defaultIface.transmit_speed) }}</span>
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
                      <Chip
                        v-for="(v,i) in (defaultIface.ipv4 ?? [])"
                        :key="'v4-'+i"
                        :label="typeof v==='string' ? v : `${v.addr}/${v.prefix_len}`"
                        class="font-mono"
                      />
                      <span v-if="(defaultIface.ipv4?.length ?? 0) === 0">-</span>
                    </div>
                  </div>
                  <div>
                    <span class="text-surface-500 text-xs">IPv6</span>
                    <div class="mt-1 flex flex-wrap gap-2">
                      <Chip
                        v-for="(v,i) in (defaultIface.ipv6 ?? [])"
                        :key="'v6-'+i"
                        :label="typeof v==='string' ? v : `${v.addr}/${v.prefix_len}`"
                        class="font-mono"
                      />
                      <span v-if="(defaultIface.ipv6?.length ?? 0) === 0">-</span>
                    </div>
                    <div class="text-xs text-surface-500 mt-2" v-if="(defaultIface.ipv6_scope_ids?.length ?? 0) > 0">
                      Scope IDs: <span class="font-mono">{{ (defaultIface.ipv6_scope_ids ?? []).join(', ') }}</span>
                    </div>
                  </div>
                </div>

                <!-- Routing / DNS -->
                <div class="rounded-xl border border-surface-200 dark:border-surface-700 p-3">
                  <div class="font-semibold mb-2">Routing / DNS</div>
                  <div class="flex flex-wrap gap-6">
                    <div>
                      <div class="text-surface-500 text-xs">Gateway</div>
                      <div class="mt-1">
                        <template v-if="defaultIface.gateway">
                          <div class="font-mono">MAC: {{ defaultIface.gateway.mac_addr }}</div>
                          <div v-if="defaultIface.gateway.ipv4.length > 0">
                            IPv4: <span class="font-mono">{{ defaultIface.gateway.ipv4.join(', ') }}</span>
                          </div>
                          <div v-if="defaultIface.gateway.ipv6.length > 0">
                            IPv6: <span class="font-mono">{{ defaultIface.gateway.ipv6.join(', ') }}</span>
                          </div>
                        </template>
                        <span v-else>-</span>
                      </div>
                    </div>
                    <div>
                      <div class="text-surface-500 text-xs">DNS</div>
                      <div class="mt-1 flex flex-wrap gap-2">
                        <Chip v-for="(d,i) in (defaultIface.dns_servers ?? [])" :key="'dns-'+i" :label="d" class="font-mono" />
                        <span v-if="(defaultIface.dns_servers?.length ?? 0) === 0">-</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div> <!-- /else -->
            </div>
          </template>
        </Card>

        <!-- Public IP -->
        <Card>
          <template #title>Public IP</template>
          <template #content>
            <div class="flex flex-col gap-4 text-sm">
              <div v-if="!ipDual" class="text-surface-500">Loading...</div>

              <div v-else class="grid grid-cols-1 gap-3">
                <div class="flex items-center gap-3">
                  <i class="pi pi-globe text-surface-500"></i>
                  <Tag
                    :value="(v4 || v6) ? 'Internet Connection' : 'No Internet'"
                    :severity="(v4 || v6) ? 'success' : 'danger'"
                  />
                </div>

                <!-- IPv4 -->
                <div class="p-3 rounded-lg border border-surface-200 dark:border-surface-700">
                  <div class="flex items-center justify-between mb-2">
                    <span class="font-semibold">IPv4</span>
                    <Tag :severity="v4 ? 'success' : 'secondary'" :value="v4 ? 'Detected' : 'n/a'" />
                  </div>
                  <div v-if="v4" class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-2 text-sm">
                    <div class="text-surface-500">Address</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(v4.ip_addr) }}</div>

                    <div class="text-surface-500">Host</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(v4.host_name)) }}</div>

                    <div class="text-surface-500">Network</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(v4.network)) }}</div>

                    <div class="text-surface-500">ASN</div>
                    <div v-if="publicIpVisible" class="font-medium">
                      {{ nv(v4.asn) }} <span class="text-surface-500">{{ nv(v4.as_name) }}</span>
                    </div>
                    <div v-else class="font-mono break-all text-surface-500">{{ pubIpGate(nv(v4.asn)) }}</div>

                    <div class="text-surface-500">Location</div>
                    <div v-if="publicIpVisible" class="font-medium">{{ nv(v4.country_name) }} ({{ nv(v4.country_code) }})</div>
                    <div v-else class="font-mono break-all text-surface-500">{{ pubIpGate(nv(v4.country_name)) }}</div>
                  </div>
                  <div v-else class="text-surface-500 text-sm">Not available.</div>
                </div>

                <!-- IPv6 -->
                <div class="p-3 rounded-lg border border-surface-200 dark:border-surface-700">
                  <div class="flex items-center justify-between mb-2">
                    <span class="font-semibold">IPv6</span>
                    <Tag :severity="v6 ? 'success' : 'secondary'" :value="v6 ? 'Detected' : 'n/a'" />
                  </div>
                  <div v-if="v6" class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-2 text-sm">
                    <div class="text-surface-500">Address</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(v6.ip_addr) }}</div>

                    <div class="text-surface-500">Host</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(v6.host_name)) }}</div>

                    <div class="text-surface-500">Network</div>
                    <div class="font-mono break-all" :class="{ 'text-surface-500': !publicIpVisible }">{{ pubIpGate(nv(v6.network)) }}</div>

                    <div class="text-surface-500">ASN</div>
                    <div v-if="publicIpVisible" class="font-medium">
                      {{ nv(v6.asn) }} <span class="text-surface-500">{{ nv(v6.as_name) }}</span>
                    </div>
                    <div v-else class="font-mono break-all text-surface-500">{{ pubIpGate(nv(v6.asn)) }}</div>

                    <div class="text-surface-500">Location</div>
                    <div v-if="publicIpVisible" class="font-medium">{{ nv(v6.country_name) }} ({{ nv(v6.country_code) }})</div>
                    <div v-else class="font-mono break-all text-surface-500">{{ pubIpGate(nv(v6.country_name)) }}</div>
                  </div>
                  <div v-else class="text-surface-500 text-sm">Not available.</div>
                </div>
              </div> <!-- /else -->
            </div>
          </template>
        </Card>
      </div>
    </ScrollPanel>
    </div>
  </div>
</template>
