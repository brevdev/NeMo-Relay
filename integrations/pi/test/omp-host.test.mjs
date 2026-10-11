// SPDX-FileCopyrightText: Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

/**
 * The extension under omp (oh-my-pi), the pi fork that loads pi extensions.
 *
 * Two things differ from pi. The extension reports to omp's own hook route, so a session is
 * attributed to omp rather than pi. And omp's `registerProvider` builds fresh model objects
 * rather than rewriting them in place, so the running session has to be pointed at the
 * redirected model or its calls keep going to the old endpoint.
 *
 * Run: node --test integrations/pi/test/*.test.mjs
 */
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { after, before, beforeEach, describe, it } from 'node:test';

import { listen } from './harness.mjs';

const { detectHost, configFromEnv } = await import('../src/gateway-client.ts');
const extension = (await import('../index.ts')).default;

describe('host detection', () => {
  it('takes the host the launcher names', () => {
    assert.equal(detectHost({ NEMO_RELAY_PI_HOST: 'omp' }, '/usr/local/bin/pi'), 'omp');
    assert.equal(detectHost({ NEMO_RELAY_PI_HOST: 'pi' }, '/opt/omp/omp'), 'pi');
  });

  it('falls back to the executable for a persistent install', () => {
    assert.equal(detectHost({}, '/opt/omp-18.8.7/omp'), 'omp');
    assert.equal(detectHost({}, 'C:\\tools\\omp.exe'), 'omp');
    assert.equal(detectHost({}, '/opt/pi-1.1.0/pi'), 'pi');
    // pi run from source under Node, and anything unrecognized, stays pi.
    assert.equal(detectHost({}, '/usr/bin/node'), 'pi');
    assert.equal(detectHost({}, '/usr/bin/ompsomething'), 'pi');
    assert.equal(detectHost({ NEMO_RELAY_PI_HOST: 'other' }, '/usr/bin/node'), 'pi');
  });

  it('posts to the detected host’s hook route', () => {
    const previous = process.env.NEMO_RELAY_PI_HOST;
    try {
      process.env.NEMO_RELAY_PI_HOST = 'omp';
      assert.equal(configFromEnv('s').hookPath, '/hooks/omp');
      process.env.NEMO_RELAY_PI_HOST = 'pi';
      assert.equal(configFromEnv('s').hookPath, '/hooks/pi');
    } finally {
      if (previous === undefined) delete process.env.NEMO_RELAY_PI_HOST;
      else process.env.NEMO_RELAY_PI_HOST = previous;
    }
  });
});

describe('redirect under omp', () => {
  const model = {
    id: 'glm-5.3-flash',
    api: 'openai-completions',
    provider: 'webster',
    baseUrl: 'https://models.example.test/v1',
  };
  const paths = [];
  let server;
  let url;

  before(async () => {
    server = createServer((req, res) => {
      paths.push(req.url);
      req.resume();
      req.on('end', () => {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end('{}');
      });
    });
    url = await listen(server);
    process.env.NEMO_RELAY_PI_GATEWAY_URL = url;
    process.env.NEMO_RELAY_PI_REDIRECT = 'force';
    process.env.NEMO_RELAY_PI_HOST = 'omp';
  });

  after(() => {
    server.close();
    delete process.env.NEMO_RELAY_PI_GATEWAY_URL;
    delete process.env.NEMO_RELAY_PI_REDIRECT;
    delete process.env.NEMO_RELAY_PI_HOST;
  });

  beforeEach(() => {
    paths.length = 0;
  });

  /** Load the extension against a host whose registry returns `registered` after a redirect. */
  const load = (registered) => {
    const handlers = new Map();
    const selected = [];
    extension({
      on: (name, handler) => handlers.set(name, [...(handlers.get(name) ?? []), handler]),
      registerProvider() {},
      setModel: async (next) => {
        selected.push(next);
        return true;
      },
    });
    const ctx = {
      cwd: '/work',
      mode: 'print',
      hasUI: false,
      sessionManager: { getSessionId: () => 'sess-omp' },
      model,
      modelRegistry: { getAll: () => [model], find: () => registered },
    };
    const fire = async (name, event = {}) => {
      for (const handler of handlers.get(name) ?? []) await handler({ type: name, ...event }, ctx);
    };
    return { fire, selected };
  };

  it('re-selects the redirected model when the host built a new one', async () => {
    const { fire, selected } = load({ ...model, baseUrl: url });
    await fire('session_start', { reason: 'startup' });
    await fire('session_shutdown', { reason: 'quit' });
    assert.equal(selected.length, 1);
    assert.equal(selected[0].baseUrl, url);
    assert.ok(paths.length > 0 && paths.every((path) => path === '/hooks/omp'), paths.join(','));
  });

  it('leaves the session alone when the model already follows the registration', async () => {
    // pi rewrites a provider's models in place, so the registry hands back the same endpoint.
    const { fire, selected } = load(model);
    await fire('session_start', { reason: 'startup' });
    await fire('session_shutdown', { reason: 'quit' });
    assert.equal(selected.length, 0);
  });
});
