-- Granular nginx / app-publishing status reported by the agent:
-- READY | NGINX_MISSING | NGINX_INACTIVE | NOT_CONFIGURED. Supersedes
-- the coarse app_publishing_ready bool for display (the bool stays as
-- the deploy-guard "ready" flag = status READY).
ALTER TABLE servers ADD COLUMN nginx_status VARCHAR(32) NULL AFTER app_publishing_ready;
