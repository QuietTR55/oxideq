# Configuration

OxideQ is configured using environment variables. You can set these in your shell or use a `.env` file in the project root.

## Environment Variables

| Variable | Description | Required | Default |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_PASSWORD` | The password required for API authentication. All clients must send this password in the `x-oxideq-password` header. | **Yes** | N/A |
| `OXIDEQ_PORT` | The port on which the HTTP server listens. | No | `8540` |

## Example `.env` file

```env
OXIDEQ_PASSWORD=super_secret_password
OXIDEQ_PORT=8080
```
