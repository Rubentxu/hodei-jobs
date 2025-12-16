asyncapi: 3.0.0
info:
  title: Hodei Job Platform Domain Events
  version: 0.1.0
  description: |
    This document describes the domain events pertinent to the Hodei Job Platform. 
    It serves as a reference for all event-driven interactions within the system.

servers:
  production:
    host: kafka:9092
    protocol: kafka
    description: Production Kafka Broker

channels:
  domain-events:
    address: hodei.domain.events
    messages:
      JobCreated:
        $ref: '#/components/messages/JobCreated'
      JobStatusChanged:
        $ref: '#/components/messages/JobStatusChanged'
      WorkerRegistered:
        $ref: '#/components/messages/WorkerRegistered'
      WorkerStatusChanged:
        $ref: '#/components/messages/WorkerStatusChanged'
      ProviderRegistered:
        $ref: '#/components/messages/ProviderRegistered'
      ProviderUpdated:
        $ref: '#/components/messages/ProviderUpdated'
      JobCancelled:
        $ref: '#/components/messages/JobCancelled'

components:
  messages:
    JobCreated:
      payload:
        $ref: '#/components/schemas/JobCreatedPayload'
      summary: Emitted when a new job is created.
    JobStatusChanged:
      payload:
        $ref: '#/components/schemas/JobStatusChangedPayload'
      summary: Emitted when a job's status transitions.
    WorkerRegistered:
      payload:
        $ref: '#/components/schemas/WorkerRegisteredPayload'
      summary: Emitted when a worker registers with the platform.
    WorkerStatusChanged:
      payload:
        $ref: '#/components/schemas/WorkerStatusChangedPayload'
      summary: Emitted when a worker's status changes.
    ProviderRegistered:
      payload:
        $ref: '#/components/schemas/ProviderRegisteredPayload'
      summary: Emitted when a new provider registers.
    ProviderUpdated:
      payload:
        $ref: '#/components/schemas/ProviderUpdatedPayload'
      summary: Emitted when a provider is updated.
    JobCancelled:
      payload:
        $ref: '#/components/schemas/JobCancelledPayload'
      summary: Emitted when a job is explicitly cancelled.

  schemas:
    JobCreatedPayload:
      type: object
      properties:
        job_id:
          type: string
          format: uuid
        spec:
          type: object
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    JobStatusChangedPayload:
      type: object
      properties:
        job_id:
          type: string
          format: uuid
        old_state:
          type: string
        new_state:
          type: string
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    WorkerRegisteredPayload:
      type: object
      properties:
        worker_id:
          type: string
          format: uuid
        provider_id:
          type: string
          format: uuid
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    WorkerStatusChangedPayload:
      type: object
      properties:
        worker_id:
          type: string
          format: uuid
        old_status:
          type: string
        new_status:
          type: string
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    ProviderRegisteredPayload:
      type: object
      properties:
        provider_id:
          type: string
          format: uuid
        provider_type:
          type: string
        config_summary:
          type: string
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    ProviderUpdatedPayload:
      type: object
      properties:
        provider_id:
          type: string
          format: uuid
        changes:
          type: string
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string

    JobCancelledPayload:
      type: object
      properties:
        job_id:
          type: string
          format: uuid
        reason:
          type: string
        occurred_at:
          type: string
          format: date-time
        correlation_id:
          type: string
        actor:
          type: string
