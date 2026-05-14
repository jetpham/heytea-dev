# SEO And Discovery Files

The site service publishes the dashboard and discovery routes directly:

- `robots.txt`
- `sitemap.xml`
- `llms.txt`
- `llms-full.txt`
- `agents.txt`
- `skill.md`
- `site.webmanifest`
- `.well-known/security.txt`
- `.well-known/ai-plugin.json`
- `.well-known/mcp.json`
- `.well-known/agent.json`
- `.well-known/webmcp.json`
- OpenGraph meta tags
- JSON-LD structured data
- root content negotiation for JSON, markdown, and plain text
- `GET /openapi.json`
- `GET /docs` via a Vite-built Swagger UI bundle

The docs domain routes `/` to `/docs` and proxies `/openapi.json` to the API service.
