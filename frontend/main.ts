import "./styles.css"

const app = document.querySelector<HTMLDivElement>("#app")

if (!app) {
  throw new Error("App root not found")
}

app.innerHTML = `
  <main class="shell">
    <section class="panel">
      <div class="eyebrow">HydroServer</div>
      <h1>Streaming Data Loader</h1>
      <p class="lede">
        Tauri shell with a minimal vanilla TypeScript frontend.
      </p>

      <div class="status-row">
        <span class="status-dot" id="sidecar-dot" aria-hidden="true"></span>
        <span id="sidecar-text">Checking sidecar status...</span>
      </div>

      <div class="actions">
        <button id="refresh-status" type="button">Refresh Status</button>
      </div>

      <dl class="facts">
        <div>
          <dt>Frontend</dt>
          <dd>TypeScript + HTML + CSS</dd>
        </div>
        <div>
          <dt>Rust Source</dt>
          <dd><code>src/</code></dd>
        </div>
        <div>
          <dt>Legacy Reference</dt>
          <dd><code>legacy-reference/</code></dd>
        </div>
      </dl>
    </section>
  </main>
`

const sidecarText = document.querySelector<HTMLSpanElement>("#sidecar-text")
const sidecarDot = document.querySelector<HTMLSpanElement>("#sidecar-dot")
const refreshButton = document.querySelector<HTMLButtonElement>("#refresh-status")

async function refreshSidecarStatus(): Promise<void> {
  if (!sidecarText || !sidecarDot) {
    return
  }

  sidecarText.textContent = "Checking sidecar status..."
  sidecarDot.dataset.state = "pending"

  try {
    const response = await fetch("http://127.0.0.1:5321/health")
    if (!response.ok) {
      throw new Error(`Unexpected status: ${response.status}`)
    }

    sidecarText.textContent = "Sidecar online"
    sidecarDot.dataset.state = "online"
  } catch {
    sidecarText.textContent = "Sidecar offline"
    sidecarDot.dataset.state = "offline"
  }
}

refreshButton?.addEventListener("click", () => {
  void refreshSidecarStatus()
})

void refreshSidecarStatus()
