import { reactive } from "vue";

import {
  createAuthFieldStates,
  type AuthFieldName,
  type Feedback,
  type FieldValidationState,
} from "../auth-submit";
import {
  createPipelineFieldStates,
  type PipelineFieldStates,
} from "../pipeline-submit";
import { getRouteFromHash, type AppRoute } from "../router";
import type {
  AppConfig,
  ConnectionState,
  ConnectionTestResponse,
  CsvPreviewResponse,
  CsvTransformerSettings,
  CsvTransformerTimestampSettings,
  HealthResponse,
  ServerConfig,
} from "../api";

export type PipelineIdentifierType = "name" | "index";

export type PipelineFormState = {
  filePath: string;
  hasHeaderRow: boolean;
  headerRow: number;
  dataStartRow: number;
  delimiter: string;
  identifierType: PipelineIdentifierType;
  timestamp: CsvTransformerTimestampSettings;
};

export type PreviewSelectionTarget =
  | "header-row"
  | "data-start-row"
  | "timestamp-column"
  | null;

export type PreviewRowSelectionTarget = Exclude<
  PreviewSelectionTarget,
  "timestamp-column" | null
>;

type UiState = {
  route: AppRoute;
  health: HealthResponse | null;
  config: AppConfig | null;
  connectionSummary: ConnectionTestResponse | null;
  loading: boolean;
  bootstrapError: string | null;
  welcomeFeedback: Feedback;
  settingsFeedback: Feedback;
  pipelineFeedback: Feedback;
  lastConnectionState: ConnectionState | null;
  pipelineForm: PipelineFormState;
  pipelinePreview: CsvPreviewResponse | null;
  authDraft: ServerConfig;
  authFieldStates: Record<AuthFieldName, FieldValidationState>;
  pipelineFieldStates: PipelineFieldStates;
  authSubmitting: boolean;
  pipelineSelectionTarget: PreviewSelectionTarget;
  pipelinePreviewRowsRequested: number;
  pipelineValidationAttempted: boolean;
  pipelineReadyForMapping: boolean;
  validatedPipelineSettings: CsvTransformerSettings | null;
};

export const PREVIEW_PAGE_SIZE = 20;
export const APP_NAME = "HydroServer Streaming Data Loader";
export const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key";

export function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
    workspace_id: "",
  };
}

export function createEmptyPipelineForm(): PipelineFormState {
  return {
    filePath: "",
    hasHeaderRow: true,
    headerRow: 1,
    dataStartRow: 2,
    delimiter: ",",
    identifierType: "name",
    timestamp: {
      key: "timestamp",
      format: "ISO8601",
      timezoneMode: "embeddedOffset",
    },
  };
}

export const state = reactive<UiState>({
  route: getRouteFromHash(),
  health: null,
  config: null,
  connectionSummary: null,
  loading: true,
  bootstrapError: null,
  welcomeFeedback: null,
  settingsFeedback: null,
  pipelineFeedback: null,
  lastConnectionState: null,
  pipelineForm: createEmptyPipelineForm(),
  pipelinePreview: null,
  authDraft: emptyServerConfig(),
  authFieldStates: createAuthFieldStates(),
  pipelineFieldStates: createPipelineFieldStates(),
  authSubmitting: false,
  pipelineSelectionTarget: null,
  pipelinePreviewRowsRequested: PREVIEW_PAGE_SIZE,
  pipelineValidationAttempted: false,
  pipelineReadyForMapping: false,
  validatedPipelineSettings: null,
});
