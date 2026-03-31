<template>
  <div class="p-6 max-w-4xl mx-auto">

    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-lg font-semibold text-base-content">Connections</h2>
        <p class="text-sm text-base-content/50 mt-0.5">Manage HydroServer connections</p>
      </div>
      <button class="btn btn-primary btn-sm" @click="openCreate">
        Add Connection
      </button>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="flex justify-center py-12">
      <span class="loading loading-spinner loading-md text-primary" />
    </div>

    <!-- Empty state -->
    <div
      v-else-if="connections.length === 0"
      class="border border-base-300 rounded-xl p-12 text-center"
    >
      <p class="text-base-content/50 text-sm">No connections yet</p>
      <button class="btn btn-primary btn-sm mt-4" @click="openCreate">
        Add your first connection
      </button>
    </div>

    <!-- Table -->
    <div v-else class="border border-base-300 rounded-xl overflow-hidden">
      <table class="table table-sm w-full">
        <thead class="bg-base-200 text-base-content/60 text-xs uppercase tracking-wide">
          <tr>
            <th class="font-medium">Name</th>
            <th class="font-medium">Host</th>
            <th class="font-medium">Auth</th>
            <th class="w-24"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="connection in connections"
            :key="connection.id"
            class="border-t border-base-300 hover:bg-base-200/50 transition-colors"
          >
            <td class="font-medium text-sm">{{ connection.name }}</td>
            <td class="text-sm text-base-content/70">{{ connection.host }}</td>
            <td>
              <span class="badge badge-sm badge-ghost">
                {{ connection.auth_type === "apikey" ? "API Key" : "User/Pass" }}
              </span>
            </td>
            <td>
              <div class="flex items-center justify-end gap-1">
                <button
                  class="btn btn-ghost btn-xs"
                  @click="openEdit(connection)"
                >
                  Edit
                </button>
                <button
                  class="btn btn-ghost btn-xs text-error"
                  @click="confirmDelete(connection)"
                >
                  Delete
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Error -->
    <div v-if="error" class="alert alert-error mt-4 text-sm">
      {{ error }}
    </div>

  </div>

  <!-- Connection Modal -->
  <dialog ref="modalEl" class="modal">
    <div class="modal-box w-full max-w-md">
      <h3 class="font-semibold text-base mb-4">
        {{ editingId ? "Edit Connection" : "Add Connection" }}
      </h3>

      <div class="flex flex-col gap-4">

        <label class="form-control">
          <div class="label pb-1">
            <span class="label-text text-sm">Name</span>
          </div>
          <input
            v-model="form.name"
            type="text"
            class="input input-bordered input-sm w-full"
            placeholder="My HydroServer"
          />
        </label>

        <label class="form-control">
          <div class="label pb-1">
            <span class="label-text text-sm">Host URL</span>
          </div>
          <input
            v-model="form.host"
            type="text"
            class="input input-bordered input-sm w-full"
            placeholder="https://hydroserver.example.com"
          />
        </label>

        <label class="form-control">
          <div class="label pb-1">
            <span class="label-text text-sm">Auth Type</span>
          </div>
          <select v-model="form.auth_type" class="select select-bordered select-sm w-full">
            <option value="apikey">API Key</option>
            <option value="userpass">Username / Password</option>
          </select>
        </label>

        <label v-if="form.auth_type === 'apikey'" class="form-control">
          <div class="label pb-1">
            <span class="label-text text-sm">API Key</span>
          </div>
          <input
            v-model="form.api_key"
            type="password"
            class="input input-bordered input-sm w-full"
            placeholder="••••••••"
          />
        </label>

        <template v-if="form.auth_type === 'userpass'">
          <label class="form-control">
            <div class="label pb-1">
              <span class="label-text text-sm">Username</span>
            </div>
            <input
              v-model="form.username"
              type="text"
              class="input input-bordered input-sm w-full"
              placeholder="username"
            />
          </label>
          <label class="form-control">
            <div class="label pb-1">
              <span class="label-text text-sm">Password</span>
            </div>
            <input
              v-model="form.password"
              type="password"
              class="input input-bordered input-sm w-full"
              placeholder="••••••••"
            />
          </label>
        </template>

      </div>

      <div v-if="formError" class="alert alert-error mt-4 text-sm py-2">
        {{ formError }}
      </div>

      <div class="modal-action mt-6">
        <button class="btn btn-ghost btn-sm" @click="closeModal">Cancel</button>
        <button
          class="btn btn-primary btn-sm"
          :class="{ loading: saving }"
          :disabled="saving"
          @click="save"
        >
          {{ editingId ? "Save Changes" : "Add Connection" }}
        </button>
      </div>
    </div>
    <form method="dialog" class="modal-backdrop">
      <button @click="closeModal">close</button>
    </form>
  </dialog>

  <!-- Delete Confirm Modal -->
  <dialog ref="deleteModalEl" class="modal">
    <div class="modal-box max-w-sm">
      <h3 class="font-semibold text-base mb-2">Delete Connection</h3>
      <p class="text-sm text-base-content/70">
        Are you sure you want to delete
        <span class="font-medium text-base-content">{{ deletingConnection?.name }}</span>?
        Any tasks using this connection will need to be reassigned.
      </p>
      <div class="modal-action mt-6">
        <button class="btn btn-ghost btn-sm" @click="deleteModalEl?.close()">Cancel</button>
        <button
          class="btn btn-error btn-sm"
          :class="{ loading: deleting }"
          :disabled="deleting"
          @click="deleteConnection"
        >
          Delete
        </button>
      </div>
    </div>
    <form method="dialog" class="modal-backdrop">
      <button>close</button>
    </form>
  </dialog>

</template>

<script setup lang="ts">
import { ref, onMounted } from "vue"
import { api, type HydroServerConnection, type ConnectionPayload } from "@/api"

// ── State ─────────────────────────────────────────────────────────────────────
const connections = ref<HydroServerConnection[]>([])
const loading = ref(false)
const error = ref<string | null>(null)

// ── Modal state ───────────────────────────────────────────────────────────────
const modalEl = ref<HTMLDialogElement | null>(null)
const editingId = ref<string | null>(null)
const saving = ref(false)
const formError = ref<string | null>(null)

const emptyForm = (): ConnectionPayload => ({
  name: "",
  host: "",
  auth_type: "apikey",
  api_key: null,
  username: null,
  password: null,
})
const form = ref<ConnectionPayload>(emptyForm())

// ── Delete state ──────────────────────────────────────────────────────────────
const deleteModalEl = ref<HTMLDialogElement | null>(null)
const deletingConnection = ref<HydroServerConnection | null>(null)
const deleting = ref(false)

// ── Load ──────────────────────────────────────────────────────────────────────
async function load() {
  loading.value = true
  error.value = null
  try {
    connections.value = await api.connections.list()
  } catch {
    error.value = "Failed to load connections."
  } finally {
    loading.value = false
  }
}

onMounted(load)

// ── Modal ─────────────────────────────────────────────────────────────────────
function openCreate() {
  editingId.value = null
  form.value = emptyForm()
  formError.value = null
  modalEl.value?.showModal()
}

function openEdit(connection: HydroServerConnection) {
  editingId.value = connection.id
  form.value = {
    name: connection.name,
    host: connection.host,
    auth_type: connection.auth_type,
    api_key: connection.api_key,
    username: connection.username,
    password: connection.password,
  }
  formError.value = null
  modalEl.value?.showModal()
}

function closeModal() {
  modalEl.value?.close()
}

async function save() {
  formError.value = null

  if (!form.value.name.trim()) {
    formError.value = "Name is required."
    return
  }
  if (!form.value.host.trim()) {
    formError.value = "Host URL is required."
    return
  }
  if (form.value.auth_type === "apikey" && !form.value.api_key?.trim()) {
    formError.value = "API key is required."
    return
  }
  if (form.value.auth_type === "userpass" && (!form.value.username?.trim() || !form.value.password?.trim())) {
    formError.value = "Username and password are required."
    return
  }

  saving.value = true
  try {
    if (editingId.value) {
      await api.connections.update(editingId.value, form.value)
    } else {
      await api.connections.create(form.value)
    }
    closeModal()
    await load()
  } catch {
    formError.value = "Failed to save. Please try again."
  } finally {
    saving.value = false
  }
}

// ── Delete ────────────────────────────────────────────────────────────────────
function confirmDelete(connection: HydroServerConnection) {
  deletingConnection.value = connection
  deleteModalEl.value?.showModal()
}

async function deleteConnection() {
  if (!deletingConnection.value) return
  deleting.value = true
  try {
    await api.connections.delete(deletingConnection.value.id)
    deleteModalEl.value?.close()
    await load()
  } catch {
    error.value = "Failed to delete connection."
  } finally {
    deleting.value = false
  }
}
</script>