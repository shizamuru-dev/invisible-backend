.PHONY: up build down logs test clean dev

# Build and start all services
up:
	docker compose up -d --build
	@echo "Waiting for API to be ready..."
	@for i in $$(seq 1 30); do \
		curl -s http://localhost:3001 > /dev/null 2>&1 && echo "API ready!" && break || true; \
		sleep 1; \
	done

# Just build images without starting
build:
	docker compose build --no-cache

# Stop all services
down:
	docker compose down

# Show logs
logs:
	docker compose logs -f

# Run integration tests
test:
	cd test_tool && npm install && API_URL=http://localhost:3001 RELAY_WS_URL=ws://localhost:3030 npm run test:all

# Clean up volumes and stopped containers
clean:
	docker compose down -v --remove-orphans
	docker image prune -f

# Development mode - build and run with fresh data
dev: clean up

# Full restart
restart: down up

# Quick rebuild api only
rebuild-api:
	docker compose build --no-cache api

# Quick rebuild relay only
rebuild-relay:
	docker compose build --no-cache relay
