#!/usr/bin/env node

import { createInterface } from 'node:readline'

const mode = process.env.WHATSAPP_STUB_MODE ?? 'normal'

if (mode === 'stale') process.exit(0)

const chats = [
  {
    id: '12025551234@s.whatsapp.net',
    name: 'Alice',
    type: 'individual',
    unread_count: 2,
    last_message: {
      id: 'seed-2',
      chat_id: '12025551234@s.whatsapp.net',
      from: '12025551234@s.whatsapp.net',
      from_name: 'Alice',
      timestamp: '2023-11-14T22:13:20.000Z',
      is_outgoing: false,
      type: 'text',
      text: 'see you soon',
    },
  },
  {
    id: '12095550000-1234@g.us',
    name: 'Team Standup',
    type: 'group',
    unread_count: 0,
    members: [{ id: '12025551234@s.whatsapp.net' }, { id: '12095550000@s.whatsapp.net' }],
  },
]

const messages = [
  {
    id: 'seed-1',
    chat_id: '12025551234@s.whatsapp.net',
    from: '12025551234@s.whatsapp.net',
    from_name: 'Alice',
    timestamp: '2023-11-14T22:13:20.000Z',
    is_outgoing: false,
    type: 'text',
    text: 'seeded hello from stub whatsapp',
  },
]

function respond(id, result) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, result })}\n`)
}

const reader = createInterface({ input: process.stdin })
reader.on('line', (line) => {
  const trimmed = line.trim()
  if (!trimmed) return
  if (mode === 'hung') return
  if (mode === 'malformed') {
    process.stdout.write('this is not json\n')
    return
  }
  let request
  try {
    request = JSON.parse(trimmed)
  } catch {
    return
  }
  const { id, method, params = {} } = request
  if (method === 'auth.status') {
    return respond(id, {
      authenticated: true,
      account: { id: '12025551234:1@s.whatsapp.net', name: 'Alice', phone_number: '12025551234' },
    })
  }
  if (method === 'auth.login') {
    return respond(id, {
      authenticated: true,
      account_id: '12025551234:1@s.whatsapp.net',
      next_action: null,
      message: null,
    })
  }
  if (method === 'chats.list') {
    let list = chats
    if (typeof params.chat_id === 'string' && params.chat_id) {
      list = list.filter((chat) => chat.id === params.chat_id)
    }
    if (typeof params.search === 'string' && params.search) {
      const needle = params.search.toLowerCase()
      list = list.filter((chat) => chat.name.toLowerCase().includes(needle) || chat.id.toLowerCase().includes(needle))
    }
    if (typeof params.limit === 'number') list = list.slice(0, params.limit)
    return respond(id, list)
  }
  if (method === 'messages.list') return respond(id, messages)
  if (method === 'message.send') {
    return respond(id, {
      id: 'sent-1',
      chat_id: params.chat_id,
      from: '',
      from_name: null,
      timestamp: '2023-11-14T22:14:00.000Z',
      is_outgoing: true,
      type: 'text',
      text: params.text,
    })
  }
  return respond(id, {})
})
