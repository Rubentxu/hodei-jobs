# Hodei Jobs gRPC API Examples

Here are the standard JSON payloads used in the `Hodei_Jobs_gRPC_Collection.json`. You can use these as templates for your requests.

## 1. JobExecutionService

### Queue Job (Simple Shell)

Method: `QueueJob`

```json
{
  "job_definition": {
    "name": "simple-shell-job",
    "command": "/bin/sh",
    "arguments": ["-c", "echo 'Hello from Postman!' && sleep 5 && echo 'Done'"],
    "requirements": {
      "cpu_cores": 1,
      "memory_bytes": 104857600
    },
    "timeout": {
      "execution_timeout": "60s"
    }
  },
  "queued_by": "postman-user"
}
```

### Queue Job (Maven Build)

Method: `QueueJob`

```json
{
  "job_definition": {
    "name": "maven-build-job",
    "command": "/bin/bash",
    "arguments": [
      "-c",
      "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && asdf plugin add java; asdf install java 17.0.6; asdf set java 17.0.6; mvn clean package -DskipTests"
    ],
    "requirements": {
      "cpu_cores": 2,
      "memory_bytes": 1073741824,
      "disk_bytes": 2147483648
    },
    "timeout": {
      "execution_timeout": "600s"
    }
  },
  "queued_by": "postman-developer"
}
```

### List Jobs

Method: `ListJobs`

```json
{
  "limit": 20,
  "offset": 0
}
```

### Get Job

Method: `GetJob`

```json
{
  "job_id": {
    "value": "YOUR_JOB_ID_HERE"
  }
}
```

## 2. LogStreamService

### Subscribe to Logs

Method: `SubscribeLogs`

```json
{
  "job_id": "YOUR_JOB_ID_HERE",
  "include_history": true,
  "tail_lines": 100
}
```

## 3. ProviderManagementService

### List Providers

Method: `ListProviders`

```json
{}
```

## 4. Test Scenarios (Error Handling)

### Queue Job with Invalid Timeout (Format Error)

Method: `QueueJob`

```json
{
  "job_definition": {
    "name": "invalid-timout-job",
    "command": "echo hello",
    "timeout": {
      "execution_timeout": "INVALID_FORMAT"
    }
  },
  "queued_by": "tester"
}
```

_Expected: gRPC Error (InvalidArgument)_

### Queue Job with Excessive Resources (Capacity Error)

Method: `QueueJob`

```json
{
  "job_definition": {
    "name": "too-big-job",
    "command": "echo hello",
    "requirements": {
      "cpu_cores": 1000,
      "memory_bytes": 999999999999
    }
  },
  "queued_by": "tester"
}
```

_Expected: gRPC Error (ResourceExhausted or similar logic)_
