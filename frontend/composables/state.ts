import { reactive } from "vue";

import {
  createAuthFieldStates,
  type AuthFieldName,
  type FieldValidationState,
} from "../auth-submit";
import {
  createPipelineFieldStates,
  type PipelineFieldStates,
} from "../pipeline-submit";
import { getRouteFromHash, type AppRoute } from "../router";
import type {
  AppConfig,
  ColumnMapping,
  ConnectionState,
  ConnectionTestResponse,
  CsvPreviewResponse,
  DatastreamSummary,
  CsvTransformerSettings,
  CsvTransformerTimestampSettings,
  HealthResponse,
  ServerConfig,
} from "../api";

export type PipelineIdentifierType = "name" | "index";
export type PipelineEditorStep = 1 | 2;

export type PipelineFormState = {
  filePath: string;
  hasHeaderRow: boolean;
  headerRow: number;
  dataStartRow: number;
  delimiter: string;
  identifierType: PipelineIdentifierType;
  timestamp: CsvTransformerTimestampSettings;
};

export type PipelineMappingDraft = {
  csvColumn: string;
  thingId: string;
  datastreamId: string;
};

export type PipelineEditTarget = {
  jobId: string;
  name: string;
  enabled: boolean;
  scheduleMinutes: number;
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
  lastConnectionState: ConnectionState | null;
  pipelineForm: PipelineFormState;
  pipelinePreview: CsvPreviewResponse | null;
  authDraft: ServerConfig;
  authFieldStates: Record<AuthFieldName, FieldValidationState>;
  pipelineFieldStates: PipelineFieldStates;
  authSubmitting: boolean;
  pipelineSelectionTarget: PreviewSelectionTarget;
  pipelineEditorStartStep: PipelineEditorStep | null;
  pipelinePreviewRowsRequested: number;
  pipelineValidationAttempted: boolean;
  pipelineReadyForMapping: boolean;
  validatedPipelineSettings: CsvTransformerSettings | null;
  pipelineDatastreams: DatastreamSummary[];
  pipelineDatastreamsLoading: boolean;
  pipelineMappingDrafts: PipelineMappingDraft[];
  validatedColumnMappings: ColumnMapping[];
  pipelineEditTarget: PipelineEditTarget | null;
  pipelineCreateSubmitting: boolean;
};

export const PREVIEW_PAGE_SIZE = 100;
export const PREVIEW_PAGE_INCREMENT = PREVIEW_PAGE_SIZE;
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
  lastConnectionState: null,
  pipelineForm: createEmptyPipelineForm(),
  pipelinePreview: null,
  authDraft: emptyServerConfig(),
  authFieldStates: createAuthFieldStates(),
  pipelineFieldStates: createPipelineFieldStates(),
  authSubmitting: false,
  pipelineSelectionTarget: null,
  pipelineEditorStartStep: null,
  pipelinePreviewRowsRequested: PREVIEW_PAGE_SIZE,
  pipelineValidationAttempted: false,
  pipelineReadyForMapping: false,
  validatedPipelineSettings: null,
  pipelineDatastreams: [],
  pipelineDatastreamsLoading: false,
  pipelineMappingDrafts: [],
  validatedColumnMappings: [],
  pipelineEditTarget: null,
  pipelineCreateSubmitting: false,
});
