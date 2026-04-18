export interface ServiceStatusResponse {
  supported: boolean
  installed: boolean
  running: boolean
  label: string
  plist_path: string
  executable_path: string
  status_message: string
}
