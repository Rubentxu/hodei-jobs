{{/*
Expand the name of the chart.
*/}}
{{- define "hodei-jobs.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "hodei-jobs.fullname" -}}
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
{{- define "hodei-jobs.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "hodei-jobs.labels" -}}
helm.sh/chart: {{ include "hodei-jobs.chart" . }}
{{ include "hodei-jobs.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "hodei-jobs.selectorLabels" -}}
app.kubernetes.io/name: {{ include "hodei-jobs.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
API selector labels
*/}}
{{- define "hodei-jobs.api.selectorLabels" -}}
{{ include "hodei-jobs.selectorLabels" . }}
app.kubernetes.io/component: api
{{- end }}

{{/*
Web selector labels
*/}}
{{- define "hodei-jobs.web.selectorLabels" -}}
{{ include "hodei-jobs.selectorLabels" . }}
app.kubernetes.io/component: web
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "hodei-jobs.serviceAccountName" -}}
{{- if .Values.api.serviceAccount.create }}
{{- default (include "hodei-jobs.fullname" .) .Values.api.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.api.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Database URL
*/}}
{{- define "hodei-jobs.databaseUrl" -}}
{{- if .Values.postgresql.enabled }}
postgres://{{ .Values.postgresql.auth.username }}:{{ .Values.postgresql.auth.password }}@{{ include "hodei-jobs.fullname" . }}-postgresql:5432/{{ .Values.postgresql.auth.database }}
{{- else }}
postgres://{{ .Values.externalDatabase.user }}:{{ .Values.externalDatabase.password }}@{{ .Values.externalDatabase.host }}:{{ .Values.externalDatabase.port }}/{{ .Values.externalDatabase.database }}
{{- end }}
{{- end }}
