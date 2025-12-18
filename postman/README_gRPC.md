# Hodei Jobs - Postman gRPC Guide

This guide helps you interact with the Hodei Jobs Platform gRPC API using Postman.

> [!IMPORTANT] > **Why is checking status 400?**
> When you import `Hodei_Jobs_gRPC_Collection.json`, Postman creates standard HTTP requests by default.
> **You cannot send gRPC requests using the standard "POST" method.**
> You MUST create a dedicated "gRPC Request" in Postman.

## Correct Workflow

1. **Import the Collection**: Use `Hodei_Jobs_gRPC_Collection.json` primarily as a **Library of Examples**.
   - It gives you the JSON Body for every request.
   - It contains Test Scripts you can copy.
2. **Create a gRPC Request**:
   - Click **New** -> **gRPC Request**.
   - **URL**: `localhost:50051`.
   - **Service Definition**: Select "Use Server Reflection" (easiest) or "Select API" (if you imported the protos).
   - **Method**: Select the method you want (e.g., `QueueJob`).
3. **Copy the Payload**:
   - Go to the imported Collection.
   - Open the request (e.g., "Queue Job").
   - Copy the **Body** content.
   - Paste it into the **Message** section of your new gRPC Request.
   - Click **Invoke**.

## Method 1: Server Reflection (Recommended)

The server has reflection enabled, which allows Postman to automatically discover services.

1. Open Postman.
2. Click **"New"** -> **"gRPC Request"**.
3. In the Server URL, enter: `localhost:50051`.
4. Click on the **"Service definition"** dropdown (it might say "Select a method").
5. Select **"Use Server Reflection"**.
   - Postman should query the server and populate the method list.
   - You can now select `hodei.JobExecutionService / QueueJob` etc.
6. Click **"Invoke"** to test.

## Method 2: Import .proto Files (Offline / Manual)

If reflection fails or you want to work offline:

1. In Postman, go to **"APIs"** in the sidebar.
2. Click **"Create an API"**.
3. Name it "Hodei Jobs gRPC".
4. Choose **"Import definition"**.
5. Select the folder `postman/grpc_protos`.
   - Ensure you import the **folder** so that imports between files work.
   - Or select all files: `hodei_all_in_one.proto`, `provider_management.proto`, `common.proto`, and the `google` folder.
6. Once imported, go back to your "gRPC Request".
7. In **"Service definition"**, select **"Select an API"** -> **"Hodei Jobs gRPC"**.

## Example request bodies

See [GRPC_EXAMPLES.md](GRPC_EXAMPLES.md) for ready-to-copy JSON payloads.

## Troubleshooting

- **"Status 400 Bad Request"**: You are sending an HTTP/1.1 POST request to a gRPC (HTTP/2) server. **Solution**: Use the "gRPC Request" type in Postman, not the standard request tab.
- **"Protocol Error"**: Ensure you use `localhost:50051` without `https` (Postman "Enable TLS" padlock should be unlocked/red).
- **"Unimplemented"**: You might be calling a service that is defined but not active in the current `server.rs` config.
