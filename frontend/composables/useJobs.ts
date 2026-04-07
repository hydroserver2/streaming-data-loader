import { deleteJob, disableJob, enableJob, listJobs, runJob } from "../api"
import { state } from "./state"

export async function refreshJobs(): Promise<void> {
  if (state.bootstrapError || state.loading) return
  // Skip polling when the tab is hidden to avoid unnecessary API calls.
  if (document.hidden) return
  try {
    state.jobs = await listJobs()
  } catch {
    // Keep existing UI state on polling failure.
  }
}

export async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

export async function handleToggleJob(jobId: string): Promise<void> {
  const job = state.jobs.find((j) => j.id === jobId)
  if (!job) return
  try {
    if (job.enabled) await disableJob(jobId)
    else await enableJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

export async function handleDeleteJob(jobId: string): Promise<void> {
  if (!window.confirm("Delete this pipeline?")) return
  try {
    await deleteJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}
