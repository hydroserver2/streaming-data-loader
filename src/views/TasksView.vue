<template>
  <div class="p-6 max-w-5xl mx-auto">

    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-lg font-semibold text-base-content">Tasks</h2>
        <p class="text-sm text-base-content/50 mt-0.5">Manage and monitor ETL tasks</p>
      </div>
      <button class="btn btn-primary btn-sm" @click="openCreate">
        Add Task
      </button>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="flex justify-center py-12">
      <span class="loading loading-spinner loading-md text-primary" />
    </div>

    <!-- Empty state -->
    <div
      v-else-if="tasks.length === 0"
      class="border border-base-300 rounded-xl p-12 text-center"
    >
      <p class="text-base-content/50 text-sm">No tasks yet</p>
      <button class="btn btn-primary btn-sm mt-4" @click="openCreate">
        Add your first task
      </button>
    </div>

    <!-- Table -->
    <div v-else class="border border-base-300 rounded-xl overflow-hidden">
      <table class="table table-sm w-full">
        <thead class="bg-base-200 text-base-content/60 text-xs uppercase tracking-wide">
          <tr>
            <th class="font-medium">Name</th>
            <th class="font-medium">Schedule</th>
            <th class="font-medium">Last Run</th>
            <th class="font-medium">Status</th>
            <th class="font-medium">Active</th>
            <th class="w-32"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="task in tasks"
            :key="task.id"
            class="border-t border-base-300 hover:bg-base-200/50 transition-colors cursor-pointer"
            @click="goToDetail(task.id)"
          >
            <td class="font-medium text-sm">{{ task.name }}</td>
            <td class="text-sm text-base-content/70">{{ formatSchedule(task.schedule) }}</td>
            <td class="text-sm text-base-content/70">{{ formatDate(task.latest_run?.started_at) }}</td>
            <td>
              <span v-if="task.latest_run" class="badge badge-sm" :class="statusBadge(task.latest_run.status)">
                {{ task.latest_run.status }}
              </span>
              <span v-else class="text-xs text-base-content/40">No runs</span>
            </td>
            <td>
              <input
                type="checkbox"
                class="toggle toggle-sm toggle-primary"
                :checked="task.is_active"
                @click.stop
                @change="toggleActive(task)"
              />
            </td>
            <td>
              <div class="flex items-center justify-end gap-1" @click.stop>
                <button
                  class="btn btn-ghost btn-xs"
                  :class="{ loading: runningTaskId === task.id }"
                  :disabled="runningTaskId === task.id"
                  @click="runNow(task)"
                >
                  Run
                </button>
                <button class="btn btn-ghost btn-xs" @click="openEdit(task)">
                  Edit
                </button>
                <button class="btn btn-ghost btn-xs text-error" @click="confirmDelete(task)">
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

  <!-- Task Modal -->
  <dialog ref="modalEl" class="modal">
    <div class="modal-box w-full max-w-2xl">
      <h3 class="font-semibold text-base mb-4">
        {{ editingId ? "Edit Task" : "Add Task" }}
      </h3>

      <div class="flex flex-col gap-4">

        <!-- Name -->
        <label class="form-control">
          <div class="label pb-1"><span class="label-text text-sm">Name</span></div>
          <input
            v-model="form.name"
            type="text"
            class="input input-bordered input-sm w-full"
            placeholder="Daily Import"
          />
        </label>

        <!-- Connection -->
        <label class="form-control">
          <div class="label pb-1"><span class="label-text text-sm">HydroServer Connection</span></div>
          <select v-model="form.connection_id" class="select select-bordered select-sm w-full">
            <option value="" disabled>Select a connection</option>
            <option v-for="c in connections" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </label>

        <!-- Source type -->
        <label class="form-control">
          <div class="label pb-1"><span class="label-text text-sm">Source Type</span></div>
          <select v-model="form.source_type" class="select select-bordered select-sm w-full">
            <option value="local">Local File</option>
            <option value="http">Remote URL</option>
          </select>
        </label>

        <!-- File path / URL -->
        <label class="form-control">
          <div class="label pb-1">
            <span class="label-text text-sm">
              {{ form.source_type === "http" ? "File URL" : "File Path" }}
            </span>
          </div>
          <input
            v-model="form.file_path"
            type="text"
            class="input input-bordered input-sm w-full"
            :placeholder="form.source_type === 'http' ? 'https://example.com/data.csv' : '/path/to/file.csv'"
          />
        </label>

        <!-- CSV settings -->
        <div class="grid grid-cols-2 gap-4">
          <label class="form-control">
            <div class="label pb-1"><span class="label-text text-sm">Delimiter</span></div>
            <input
              v-model="form.csv_delimiter"
              type="text"
              class="input input-bordered input-sm w-full"
              placeholder=","
              maxlength="1"
            />
          </label>
          <label class="form-control">
            <div class="label pb-1"><span class="label-text text-sm">Header Row Index</span></div>
            <input
              v-model.number="form.csv_header_row"
              type="number"
              class="input input-bordered input-sm w-full"
              min="0"
            />
          </label>
        </div>

        <!-- Timestamp settings -->
        <div class="grid grid-cols-2 gap-4">
          <label class="form-control">
            <div class="label pb-1"><span class="label-text text-sm">Timestamp Column</span></div>
            <input
              v-model="form.csv_timestamp_column"
              type="text"
              class="input input-bordered input-sm w-full"
              placeholder="timestamp"
            />
          </label>
          <label class="form-control">
            <div class="label pb-1"><span class="label-text text-sm">Timestamp Format</span></div>
            <input
              v-model="form.csv_timestamp_format"
              type="text"
              class="input input-bordered input-sm w-full"
              placeholder="%Y-%m-%d %H:%M:%S"
            />
          </label>
        </div>

        <!-- Column mappings -->
        <div>
          <div class="label pb-1 flex items-center justify-between">
            <span class="label-text text-sm">Column Mappings</span>
            <button class="btn btn-ghost btn-xs" @click="addMapping">+ Add</button>
          </div>
          <div class="flex flex-col gap-2">
            <div
              v-for="(mapping, i) in form.column_mappings"
              :key="i"
              class="flex items-center gap-2"
            >
              <input
                v-model="mapping.csv_column"
                type="text"
                class="input input-bordered input-sm flex-1"
                placeholder="CSV column"
              />
              <span class="text-base-content/40 text-xs">→</span>
              <input
                v-model="mapping.datastream_id"
                type="text"
                class="input input-bordered input-sm flex-1"
                placeholder="Datastream ID"
              />
              <button class="btn btn-ghost btn-xs text-error" @click="removeMapping(i)">✕</button>
            </div>
            <p v-if="form.column_mappings.length === 0" class="text-xs text-base-content/40 py-1">
              No mappings added yet
            </p>
          </div>
        </div>

        <!-- Schedule -->
        <div>
          <div class="label pb-1"><span class="label-text text-sm">Schedule</span></div>
          <div class="grid grid-cols-3 gap-3">
            <label class="form-control">
              <div class="label pb-1"><span class="label-text text-xs text-base-content/60">Every</span></div>
              <input
                v-model.number="form.schedule.interval"
                type="number"
                class="input input-bordered input-sm w-full"
                min="1"
              />
            </label>
            <label class="form-control">
              <div class="label pb-1"><span class="label-text text-xs text-base-content/60">Period</span></div>
              <select v-model="form.schedule.period" class="select select-bordered select-sm w-full">
                <option value="minutes">Minutes</option>
                <option value="hours">Hours</option>
                <option value="days">Days</option>
              </select>
            </label>
            <label class="form-control">
              <div class="label pb-1"><span class="label-text text-xs text-base-content/60">Start Time</span></div>
              <input
                v-model="form.schedule.start_time"
                type="time"
                class="input input-bordered input-sm w-full"
              />
            </label>
          </div>
        </div>

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
          {{ editingId ? "Save Changes" : "Add Task" }}
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
      <h3 class="font-semibold text-base mb-2">Delete Task</h3>
      <p class="text-sm text-base-content/70">
        Are you sure you want to delete
        <span class="font-medium text-base-content">{{ deletingTask?.name }}</span>?
        All run history will also be deleted.
      </p>
      <div class="modal-action mt-6">
        <button class="btn btn-ghost btn-sm" @click="deleteModalEl?.close()">Cancel</button>
        <button
          class="btn btn-error btn-sm"
          :class="{ loading: deleting }"
          :disabled="deleting"
          @click="deleteTask"
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
import { useRouter } from "vue-router"
import { api, type Task, type TaskPayload, type HydroServerConnection, type Schedule } from "@/api"

const router = useRouter()

// ── State ─────────────────────────────────────────────────────────────────────
const tasks = ref<Task[]>([])
const connections = ref<HydroServerConnection[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const runningTaskId = ref<string | null>(null)

// ── Modal state ───────────────────────────────────────────────────────────────
const modalEl = ref<HTMLDialogElement | null>(null)
const editingId = ref<string | null>(null)
const saving = ref(false)
const formError = ref<string | null>(null)

const emptySchedule = (): Schedule => ({
  period: "hours",
  interval: 1,
  start_time: "00:00",
})

const emptyForm = (): TaskPayload => ({
  name: "",
  connection_id: "",
  source_type: "local",
  file_path: "",
  csv_delimiter: ",",
  csv_header_row: 0,
  csv_timestamp_column: "",
  csv_timestamp_format: "%Y-%m-%d %H:%M:%S",
  column_mappings: [],
  schedule: emptySchedule(),
  is_active: true,
})

const form = ref<TaskPayload>(emptyForm())

// ── Delete state ──────────────────────────────────────────────────────────────
const deleteModalEl = ref<HTMLDialogElement | null>(null)
const deletingTask = ref<Task | null>(null)
const deleting = ref(false)

// ── Load ──────────────────────────────────────────────────────────────────────
async function load() {
  loading.value = true
  error.value = null
  try {
    ;[tasks.value, connections.value] = await Promise.all([
      api.tasks.list(),
      api.connections.list(),
    ])
  } catch {
    error.value = "Failed to load tasks."
  } finally {
    loading.value = false
  }
}

onMounted(load)

// ── Navigation ────────────────────────────────────────────────────────────────
function goToDetail(id: string) {
  router.push({ name: "task-detail", params: { id } })
}

// ── Formatters ────────────────────────────────────────────────────────────────
function formatSchedule(schedule: Schedule | null | undefined): string {
  if (!schedule) return "No schedule"
  return `Every ${schedule.interval} ${schedule.period}`
}

function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—"
  return new Date(iso).toLocaleString()
}

function statusBadge(status: string): string {
  return {
    success: "badge-success",
    failure: "badge-error",
    started: "badge-warning",
  }[status] ?? "badge-ghost"
}

// ── Column mappings ───────────────────────────────────────────────────────────
function addMapping() {
  form.value.column_mappings = [
    ...(form.value.column_mappings ?? []),
    { csv_column: "", datastream_id: "" },
  ]
}

function removeMapping(index: number) {
  form.value.column_mappings = form.value.column_mappings?.filter((_, i) => i !== index)
}

// ── Modal ─────────────────────────────────────────────────────────────────────
function openCreate() {
  editingId.value = null
  form.value = emptyForm()
  formError.value = null
  modalEl.value?.showModal()
}

function openEdit(task: Task) {
  editingId.value = task.id
  form.value = {
    name: task.name,
    connection_id: task.connection_id,
    source_type: task.source_type,
    file_path: task.file_path,
    csv_delimiter: task.csv_delimiter,
    csv_header_row: task.csv_header_row,
    csv_timestamp_column: task.csv_timestamp_column,
    csv_timestamp_format: task.csv_timestamp_format,
    column_mappings: task.column_mappings ? [...task.column_mappings] : [],
    schedule: task.schedule ? { ...task.schedule } : emptySchedule(),
    is_active: task.is_active,
  }
  formError.value = null
  modalEl.value?.showModal()
}

function closeModal() {
  modalEl.value?.close()
}

function validate(): string | null {
  if (!form.value.name.trim()) return "Name is required."
  if (!form.value.connection_id) return "Connection is required."
  if (!form.value.file_path.trim()) return "File path or URL is required."
  if (!form.value.csv_timestamp_column.trim()) return "Timestamp column is required."
  if (!form.value.csv_timestamp_format.trim()) return "Timestamp format is required."
  if (form.value.schedule) {
    if (!form.value.schedule.interval || form.value.schedule.interval <= 0)
      return "Schedule interval must be a positive number."
  }
  return null
}

async function save() {
  formError.value = validate()
  if (formError.value) return

  saving.value = true
  try {
    if (editingId.value) {
      await api.tasks.update(editingId.value, form.value)
    } else {
      await api.tasks.create(form.value)
    }
    closeModal()
    await load()
  } catch {
    formError.value = "Failed to save task. Please try again."
  } finally {
    saving.value = false
  }
}

// ── Toggle active ─────────────────────────────────────────────────────────────
async function toggleActive(task: Task) {
  try {
    await api.tasks.update(task.id, {
      name: task.name,
      connection_id: task.connection_id,
      source_type: task.source_type,
      file_path: task.file_path,
      csv_delimiter: task.csv_delimiter,
      csv_header_row: task.csv_header_row,
      csv_timestamp_column: task.csv_timestamp_column,
      csv_timestamp_format: task.csv_timestamp_format,
      column_mappings: task.column_mappings,
      schedule: task.schedule,
      is_active: !task.is_active,
    })
    await load()
  } catch {
    error.value = "Failed to update task."
  }
}

// ── Run now ───────────────────────────────────────────────────────────────────
async function runNow(task: Task) {
  runningTaskId.value = task.id
  try {
    await api.tasks.runNow(task.id)
    await load()
  } catch {
    error.value = "Failed to trigger run."
  } finally {
    runningTaskId.value = null
  }
}

// ── Delete ────────────────────────────────────────────────────────────────────
function confirmDelete(task: Task) {
  deletingTask.value = task
  deleteModalEl.value?.showModal()
}

async function deleteTask() {
  if (!deletingTask.value) return
  deleting.value = true
  try {
    await api.tasks.delete(deletingTask.value.id)
    deleteModalEl.value?.close()
    await load()
  } catch {
    error.value = "Failed to delete task."
  } finally {
    deleting.value = false
  }
}
</script>