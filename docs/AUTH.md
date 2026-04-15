# Authentication

The `auth` service handles simple user registration and login, returning JSON Web Tokens (JWT) for authentication.

## API Endpoints

### `POST /api/auth/register`
Registers a new user in the system.

**Request:**
```json
{
  "username": "alice",
  "password": "supersecretpassword123"
}
```

**Responses:**
- `201 Created`: User created successfully.
- `400 Bad Request`: Username or password missing.
- `409 Conflict`: Username already exists.

### `POST /api/auth/login`
Authenticates a user and returns a JWT token.

**Request:**
```json
{
  "username": "alice",
  "password": "supersecretpassword123"
}
```

**Responses:**
- `200 OK`:
  ```json
  {
    "token": "eyJ0eXAi... (JWT Token)"
  }
  ```
- `401 Unauthorized`: Invalid username or password.