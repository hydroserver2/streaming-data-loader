import { createRouter, createWebHistory } from "vue-router"
import ConnectionsView from "@/views/ConnectionsView.vue"
import TasksView from "@/views/TasksView.vue"
import TaskDetailView from "@/views/TaskDetailView.vue"

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/connections" },
    { path: "/connections", name: "connections", component: ConnectionsView },
    { path: "/tasks", name: "tasks", component: TasksView },
    { path: "/tasks/:id", name: "task-detail", component: TaskDetailView },
  ],
})

export default router