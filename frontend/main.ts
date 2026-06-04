import "./generated.css"

import { createApp } from "vue"

import App from "./App.vue"
import { isWindowsPlatform } from "./api/runtime"

if (typeof document !== "undefined" && isWindowsPlatform()) {
  document.documentElement.classList.add("platform-windows")
}

createApp(App).mount("#app")
