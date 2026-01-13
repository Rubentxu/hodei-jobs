{{/*
Expand the name of the chart.
*/}}
{{- define "hodei-jobs-platform.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "hodei-jobs-platform.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "hodei-jobs-platform.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "hodei-jobs-platform.labels" -}}
helm.sh/chart: {{ include "hodei-jobs-platform.chart" . }}
{{ include "hodei-jobs-platform.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
hodei.io/component: "hodei-jobs-platform"
{{- end }}

{{/*
Selector labels
*/}}
{{- define "hodei-jobs-platform.selectorLabels" -}}
app.kubernetes.io/name: {{ include "hodei-jobs-platform.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "hodei-jobs-platform.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "hodei-jobs-platform.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Provider configuration name
*/}}
{{- define "hodei-jobs-platform.providerConfigName" -}}
{{- printf "%s-provider-config" (include "hodei-jobs-platform.fullname" .) }}
{{- end }}

{{/*
Kubernetes worker namespace
*/}}
{{- define "hodei-jobs-platform.workerNamespace" -}}
{{- .Values.kubernetesProvider.namespace }}
{{- end }}

{{/*
Network policy name
*/}}
{{- define "hodei-jobs-platform.networkPolicyName" -}}
{{- printf "%s-network-policy" (include "hodei-jobs-platform.fullname" .) }}
{{- end }}
