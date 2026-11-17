-- Default configuration (not metric data). Users can edit all of it in Settings.

INSERT OR IGNORE INTO alert_rules (id, category, enabled, threshold, duration_s, cooldown_s, updated_at) VALUES
    ('cpu_high',        'system', 1, 90,      60,  900,  0),
    ('memory_high',     'system', 1, 90,      60,  900,  0),
    ('memory_pressure', 'system', 1, 2,       30,  900,  0),   -- 2 = warning, 4 = critical (kern.memorystatus_vm_pressure_level)
    ('battery_low',     'system', 1, 15,      0,   1800, 0),
    ('disk_full',       'system', 1, 90,      0,   21600, 0),
    ('tokens_daily',    'ai',     0, 1000000, 0,   3600, 0),
    ('cost_daily',      'ai',     0, 10,      0,   3600, 0),
    ('cost_monthly',    'ai',     0, 100,     0,   21600, 0),
    ('provider_error',  'ai',     1, 1,       0,   3600, 0),
    ('auth_error',      'ai',     1, 1,       0,   21600, 0);

INSERT OR IGNORE INTO ai_providers (id, display_name, enabled, sync_interval_minutes, created_at, updated_at) VALUES
    ('openai',     'OpenAI',                 0, 60, 0, 0),
    ('anthropic',  'Anthropic Claude',       0, 60, 0, 0),
    ('gemini',     'Google Gemini',          0, 60, 0, 0),
    ('openrouter', 'OpenRouter',             0, 30, 0, 0),
    ('deepseek',   'DeepSeek',               0, 60, 0, 0),
    ('groq',       'Groq',                   0, 60, 0, 0),
    ('custom',     'Custom (OpenAI-compatible)', 0, 60, 0, 0);

-- Anthropic first-party list prices, USD / 1M tokens, from Anthropic's model table cached 2026-11-03.
-- Cache read = documented per-model value where given, otherwise 0.1x input; cache write (5 min) = 1.25x input.
-- Prices for other providers are intentionally NOT seeded: enter them in Settings → AI Providers → Pricing.
INSERT OR IGNORE INTO ai_models (provider, model, input_price, output_price, cached_input_price, cache_write_price, price_source, updated_at) VALUES
    ('anthropic', 'claude-fable-5-2',  8.00, 40.00, 0.20, 10.00, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-fable-5-1', 10.00, 50.00, 1.00, 12.50, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-fable-5',   10.00, 50.00, 1.00, 12.50, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-opus-5-5',   4.00, 20.00, 0.20,  5.00, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-opus-5',     5.00, 25.00, 0.50,  6.25, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-opus-4-8',   5.00, 25.00, 0.50,  6.25, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-opus-4-7',   5.00, 25.00, 0.50,  6.25, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-opus-4-6',   5.00, 25.00, 0.50,  6.25, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-sonnet-5-5', 2.00, 10.00, 0.10,  2.50, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-sonnet-5',   2.00, 10.00, 0.20,  2.50, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-sonnet-4-6', 3.00, 15.00, 0.30,  3.75, 'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-haiku-5-5',  0.10,  0.50, 0.01,  0.125,'anthropic-pricing@2026-11-03', 0),
    ('anthropic', 'claude-haiku-4-5',  1.00,  5.00, 0.10,  1.25, 'anthropic-pricing@2026-11-03', 0);
