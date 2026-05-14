# SEO And Discovery Files

The frontend publishes:

- `robots.txt`
- `sitemap.xml`
- `llms.txt`
- `site.webmanifest`
- `.well-known/security.txt`
- `.well-known/ai-plugin.json`
- `.well-known/mcp.json`
- OpenGraph and Twitter card meta tags
- JSON-LD WebSite metadata

The public API publishes:

- `GET /openapi.json`
- `GET /docs` via Scalar-compatible OpenAPI rendering

The docs domain should serve the same OpenAPI schema and written API docs.
