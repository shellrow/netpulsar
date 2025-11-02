<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  SocketInfo,
  ProtocolSocketInfo,
  TcpSocketInfo,
  UdpSocketInfo,
  ProcessEntry,
} from "../types/socket";

const wrapRef = ref<HTMLElement | null>(null);
const toolbarRef = ref<HTMLElement | null>(null);
const tableHeight = ref("400px");
let ro: ResizeObserver | null = null;
let rafId: number | null = null;
let scheduled = false;

const loading = ref(false);
const sockets = ref<SocketInfo[]>([]);
const q = ref("");
const family = ref<"All" | "Ipv4" | "Ipv6">("All");
const proto = ref<"All" | "TCP" | "UDP">("All");

function _calcTableHeight(): string {
  const wrap = wrapRef.value;
  if (!wrap) return tableHeight.value;

  const cs = getComputedStyle(wrap);
  const padY = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  const inner = wrap.clientHeight - padY;

  const toolbarH = toolbarRef.value?.offsetHeight ?? 0;
  const EXTRA_OFFSET = 80; // space for paginator/footer etc
  const GAP = 12;
  const px = Math.max(200, inner - toolbarH - EXTRA_OFFSET - GAP);
  return `${Math.floor(px)}px`;
}

function scheduleRecalc() {
  if (scheduled) return;
  scheduled = true;
  if (rafId) cancelAnimationFrame(rafId);
  rafId = requestAnimationFrame(() => {
    scheduled = false;
    const next = _calcTableHeight();
    if (next !== tableHeight.value) tableHeight.value = next;
  });
}

// netsock protocol helpers
function isTcp(p: ProtocolSocketInfo): p is { Tcp: TcpSocketInfo } {
  return (p as any).Tcp != null;
}
function isUdp(p: ProtocolSocketInfo): p is { Udp: UdpSocketInfo } {
  return (p as any).Udp != null;
}
function addrFamily(addr: string): "Ipv4" | "Ipv6" {
  // simple heuristic: IPv6 contains ':'
  return addr.includes(":") ? "Ipv6" : "Ipv4";
}
function anyAddr(addr: string): boolean {
  // treat wildcard-ish addresses
  return addr === "0.0.0.0" || addr === "::" || addr === "::0";
}
function fmtAddrPort(addr: string, port: number): string {
  if (addr.includes(":")) {
    // IPv6 needs brackets when showing port
    return `[${addr}]:${port}`;
  }
  return `${addr}:${port}`;
}
function protoLabel(p: ProtocolSocketInfo): "TCP" | "UDP" {
  return isTcp(p) ? "TCP" : "UDP";
}
function localTuple(si: ProtocolSocketInfo): [string, number] {
  return isTcp(si)
    ? [si.Tcp.local_addr, si.Tcp.local_port]
    : [si.Udp.local_addr, si.Udp.local_port];
}
function remoteTuple(si: ProtocolSocketInfo): [string | null, number | null] {
  if (isTcp(si)) return [si.Tcp.remote_addr, si.Tcp.remote_port];
  return [null, null];
}
function tcpState(si: ProtocolSocketInfo): string | null {
  return isTcp(si) ? si.Tcp.state : null;
}
function rowKey(s: SocketInfo): string {
  const p = s.protocol_socket_info;
  if (isTcp(p)) {
    return `T-${p.Tcp.local_addr}:${p.Tcp.local_port}-${p.Tcp.remote_addr}:${p.Tcp.remote_port}-${s.inode}-${s.uid}`;
  }
  return `U-${p.Udp.local_addr}:${p.Udp.local_port}-${s.inode}-${s.uid}`;
}

async function fetchSockets() {
  loading.value = true;
  try {
    sockets.value = (await invoke("get_sockets_all")) as SocketInfo[];
  } finally {
    loading.value = false;
  }
}

// ---------- filtering / search ----------
const filtered = computed(() => {
  let xs = sockets.value;

  // protocol filter
  if (proto.value !== "All") {
    xs = xs.filter((s) =>
      proto.value === "TCP"
        ? isTcp(s.protocol_socket_info)
        : isUdp(s.protocol_socket_info)
    );
  }

  // family filter (based on local address family)
  if (family.value !== "All") {
    xs = xs.filter((s) => {
      const [la] = localTuple(s.protocol_socket_info);
      return addrFamily(la) === family.value;
    });
  }

  // search
  const s = q.value.trim().toLowerCase();
  if (!s) return xs;

  return xs.filter((skt) => {
    const psi = skt.protocol_socket_info;
    const [la, lp] = localTuple(psi);
    const [ra, rp] = remoteTuple(psi);
    const state = tcpState(psi) ?? "";

    const proc = (skt.processes ?? [])
      .map((p: ProcessEntry) => `${p.name} ${p.pid}`)
      .join(" ");

    const hay = [
      protoLabel(psi),
      la,
      String(lp),
      ra ?? "",
      rp != null ? String(rp) : "",
      state,
      String(skt.uid),
      String(skt.inode),
      proc,
    ]
      .join(" ")
      .toLowerCase();

    return hay.includes(s);
  });
});

onMounted(async () => {
  await fetchSockets();
  await nextTick();
  tableHeight.value = _calcTableHeight();

  ro = new ResizeObserver(() => scheduleRecalc());
  if (wrapRef.value) ro.observe(wrapRef.value);
  if (toolbarRef.value) ro.observe(toolbarRef.value);
  window.addEventListener("resize", scheduleRecalc);
});

onBeforeUnmount(() => {
  ro?.disconnect();
  if (rafId) cancelAnimationFrame(rafId);
  window.removeEventListener("resize", scheduleRecalc);
});
</script>

<template>
  <div ref="wrapRef" class="p-3 lg:p-4 flex flex-col gap-3 h-full min-h-0">
    <!-- Toolbar -->
    <div
    ref="toolbarRef"
    class="grid grid-cols-1 lg:grid-cols-[1fr_auto] items-center gap-2"
    >
    <div class="flex items-center gap-3 min-w-0">
        <span class="text-surface-500 dark:text-surface-400 text-sm">
        Socket Connections ({{ filtered.length }})
        </span>
    </div>

    <div class="flex items-center gap-2 justify-end flex-nowrap min-w-0">
        <Select
        v-model="proto"
        :options="[
            { label: 'All', value: 'All' },
            { label: 'TCP', value: 'TCP' },
            { label: 'UDP', value: 'UDP' },
        ]"
        optionLabel="label"
        optionValue="value"
        class="w-24 shrink-0"
        />
        <Select
        v-model="family"
        :options="[
            { label: 'All', value: 'All' },
            { label: 'IPv4', value: 'Ipv4' },
            { label: 'IPv6', value: 'Ipv6' },
        ]"
        optionLabel="label"
        optionValue="value"
        class="w-24 shrink-0"
        />
        <div class="flex items-center gap-2 min-w-0">
        <InputGroup class="max-w-[260px] w-full">
            <InputGroupAddon><i class="pi pi-search" /></InputGroupAddon>
            <InputText
            v-model="q"
            placeholder="Search (addr/port/proc/state...)"
            class="w-full"
            />
        </InputGroup>
        <Button
            outlined
            icon="pi pi-refresh"
            :loading="loading"
            @click="fetchSockets"
            class="w-9 h-9 shrink-0"
        />
        </div>
    </div>
    </div>

    <!-- Table -->
    <DataTable
      :value="filtered"
      :loading="loading"
      :rowKey="rowKey"
      paginator
      :rows="20"
      :rowsPerPageOptions="[20, 50, 100]"
      scrollable
      :scrollHeight="tableHeight"
      class="text-sm"
      stripedRows
    >
      <Column header="Proto" style="width: 90px">
        <template #body="{ data }">
          <Tag
            :value="data.protocol_socket_info.Tcp ? 'TCP' : 'UDP'"
            severity="secondary"
          />
        </template>
      </Column>

      <Column header="Family" style="width: 90px">
        <template #body="{ data }">
          <template v-if="data.protocol_socket_info.Tcp">
            {{ data.protocol_socket_info.Tcp.local_addr.includes(':') ? 'IPv6' : 'IPv4' }}
          </template>
          <template v-else>
            {{ data.protocol_socket_info.Udp.local_addr.includes(':') ? 'IPv6' : 'IPv4' }}
          </template>
        </template>
      </Column>

      <Column header="Local" style="min-width: 220px">
        <template #body="{ data }">
          <template v-if="data.protocol_socket_info.Tcp">
            {{ fmtAddrPort(data.protocol_socket_info.Tcp.local_addr, data.protocol_socket_info.Tcp.local_port) }}
          </template>
          <template v-else>
            {{ fmtAddrPort(data.protocol_socket_info.Udp.local_addr, data.protocol_socket_info.Udp.local_port) }}
          </template>
        </template>
      </Column>

      <Column header="Remote" style="min-width: 220px">
        <template #body="{ data }">
          <template v-if="data.protocol_socket_info.Tcp">
            <template
              v-if="!(data.protocol_socket_info.Tcp.remote_port === 0 && anyAddr(data.protocol_socket_info.Tcp.remote_addr))"
            >
              {{ fmtAddrPort(data.protocol_socket_info.Tcp.remote_addr!, data.protocol_socket_info.Tcp.remote_port!) }}
            </template>
            <span v-else>-</span>
          </template>
          <span v-else>-</span>
        </template>
      </Column>

      <Column header="State" style="width: 120px">
        <template #body="{ data }">
          <span v-if="data.protocol_socket_info.Tcp">{{ data.protocol_socket_info.Tcp.state }}</span>
          <span v-else>-</span>
        </template>
      </Column>

      <Column header="Process" style="min-width: 220px">
        <template #body="{ data }">
          <div v-if="data.processes?.length">
            <span
              v-for="p in data.processes"
              :key="p.pid"
              class="inline-flex items-center gap-1 mr-2 mb-1"
            >
              <Tag :value="p.name" severity="secondary" />
              <span class="text-surface-500 text-xs">({{ p.pid }})</span>
            </span>
          </div>
          <span v-else>-</span>
        </template>
      </Column>

      <Column field="uid" header="UID" style="width: 100px" />
    </DataTable>
  </div>
</template>
