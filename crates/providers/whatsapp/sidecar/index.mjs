#!/usr/bin/env node

import { existsSync } from 'node:fs'
import { mkdir, readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { createInterface } from 'node:readline'

for (const method of ['log', 'info', 'warn', 'error', 'debug', 'trace']) {
  console[method] = (...args) => {
    const line = args.map((arg) => (typeof arg === 'string' ? arg : JSON.stringify(arg))).join(' ')
    process.stderr.write(`${line}\n`)
  }
}

const MAX_MESSAGES_PER_CHAT = 500
const SOCKET_TIMEOUT_MS = 60_000

const state = {
  sock: null,
  authDir: null,
  chats: new Map(),
  contacts: new Map(),
  messages: new Map(),
  groups: new Map(),
}

function silentLogger() {
  const logger = { level: 'silent', child: () => logger }
  const noop = () => {}
  for (const level of ['trace', 'debug', 'info', 'warn', 'error', 'fatal']) {
    logger[level] = noop
  }
  return logger
}

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`)
}

function toTimestampMs(value) {
  if (value === null || value === undefined) return 0
  if (typeof value === 'number') return value * 1000
  if (typeof value === 'object' && typeof value.toNumber === 'function') return value.toNumber() * 1000
  const numeric = Number(value)
  return Number.isNaN(numeric) ? 0 : numeric * 1000
}

function toIsoString(value) {
  return new Date(toTimestampMs(value)).toISOString()
}

function resolveJid(input) {
  const value = String(input ?? '')
  if (value.includes('@')) return value
  return `${value.replace(/[^0-9]/g, '')}@s.whatsapp.net`
}

function jidToType(jid) {
  if (jid.endsWith('@g.us')) return 'group'
  if (jid.endsWith('@broadcast')) return jid === 'status@broadcast' ? 'status' : 'broadcast'
  return 'individual'
}

function extractMessageText(message) {
  if (!message) return undefined
  if (message.conversation) return message.conversation
  if (message.extendedTextMessage?.text) return message.extendedTextMessage.text
  if (message.imageMessage?.caption) return message.imageMessage.caption
  if (message.videoMessage?.caption) return message.videoMessage.caption
  if (message.documentMessage?.caption) return message.documentMessage.caption
  if (message.contactMessage?.displayName) return message.contactMessage.displayName
  if (message.locationMessage?.name) return message.locationMessage.name
  if (message.reactionMessage?.text) return message.reactionMessage.text
  if (message.pollCreationMessage?.name) return message.pollCreationMessage.name
  return undefined
}

function getMessageType(message) {
  if (!message) return 'unknown'
  if (message.conversation || message.extendedTextMessage) return 'text'
  if (message.imageMessage) return 'image'
  if (message.videoMessage) return message.videoMessage.gifPlayback ? 'gif' : 'video'
  if (message.audioMessage) return 'audio'
  if (message.documentMessage) return 'document'
  if (message.stickerMessage) return 'sticker'
  if (message.locationMessage || message.liveLocationMessage) return 'location'
  if (message.contactMessage || message.contactsArrayMessage) return 'contact'
  if (message.reactionMessage) return 'reaction'
  if (message.pollCreationMessage) return 'poll'
  if (message.pollUpdateMessage) return 'poll_update'
  return 'unknown'
}

function messageContent(message) {
  return message.message?.protocolMessage?.editedMessage ?? message.message
}

function summarizeMessage(message) {
  const jid = message.key?.remoteJid ?? ''
  const isGroup = jid.endsWith('@g.us')
  const from = message.key?.fromMe ? '' : isGroup ? (message.key?.participant ?? message.participant ?? jid) : jid
  const content = messageContent(message)
  return {
    id: message.key?.id ?? '',
    chat_id: jid,
    from,
    from_name: message.pushName ?? undefined,
    timestamp: toIsoString(message.messageTimestamp),
    is_outgoing: Boolean(message.key?.fromMe),
    type: getMessageType(content),
    text: extractMessageText(content),
  }
}

function summarizeChat(chat, lastMessage, members) {
  const id = chat.id ?? ''
  const summary = {
    id,
    name: chat.name ?? chat.displayName ?? id,
    type: jidToType(id),
    unread_count: chat.unreadCount ?? 0,
  }
  if (lastMessage) summary.last_message = lastMessage
  if (members && members.length > 0) summary.members = members
  return summary
}

function bufferMessage(message) {
  const jid = message.key?.remoteJid
  if (!jid) return
  const list = state.messages.get(jid) ?? []
  if (list.some((existing) => existing.key?.id === message.key?.id)) return
  list.push(message)
  if (list.length > MAX_MESSAGES_PER_CHAT) list.splice(0, list.length - MAX_MESSAGES_PER_CHAT)
  state.messages.set(jid, list)
}

function waitForConnection(sock) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      sock.ev.off('connection.update', onUpdate)
      reject(new Error('timed out waiting for the WhatsApp connection to open'))
    }, SOCKET_TIMEOUT_MS)
    const onUpdate = (update) => {
      if (update.connection === 'open') {
        clearTimeout(timer)
        sock.ev.off('connection.update', onUpdate)
        resolve()
      } else if (update.connection === 'close') {
        const code = update.lastDisconnect?.error?.output?.statusCode
        if (code === 401 || code === 403) {
          clearTimeout(timer)
          sock.ev.off('connection.update', onUpdate)
          reject(new Error(code === 401 ? 'logged out; re-link the device' : 'account restricted by WhatsApp'))
        }
      }
    }
    sock.ev.on('connection.update', onUpdate)
  })
}

async function connect(authDir) {
  if (!authDir) throw Object.assign(new Error('auth_dir is required'), { code: -32002 })
  if (state.sock && state.authDir === authDir) return state.sock
  const baileys = await import('@whiskeysockets/baileys')
  const logger = silentLogger()
  const { state: authState, saveCreds } = await baileys.useMultiFileAuthState(authDir)
  const { version } = await baileys.fetchLatestBaileysVersion()
  const sock = baileys.default({
    auth: { creds: authState.creds, keys: baileys.makeCacheableSignalKeyStore(authState.keys, logger) },
    version,
    logger,
    syncFullHistory: true,
    markOnlineOnConnect: false,
    generateHighQualityLinkPreview: false,
    getMessage: async () => undefined,
  })
  state.sock = sock
  state.authDir = authDir
  state.chats = new Map()
  state.contacts = new Map()
  state.messages = new Map()
  state.groups = new Map()
  sock.ev.on('creds.update', saveCreds)
  sock.ev.on('chats.upsert', (list) => {
    for (const chat of list) if (chat.id) state.chats.set(chat.id, chat)
  })
  sock.ev.on('chats.update', (list) => {
    for (const update of list) {
      if (!update.id) continue
      const existing = state.chats.get(update.id)
      state.chats.set(update.id, existing ? { ...existing, ...update } : update)
    }
  })
  sock.ev.on('messaging-history.set', ({ chats, messages }) => {
    for (const chat of chats) if (chat.id) state.chats.set(chat.id, chat)
    for (const message of messages) bufferMessage(message)
  })
  sock.ev.on('messages.upsert', ({ messages }) => {
    for (const message of messages) bufferMessage(message)
  })
  sock.ev.on('contacts.upsert', (list) => {
    for (const contact of list) state.contacts.set(contact.id, contact)
  })
  await waitForConnection(sock)
  return sock
}

async function collectChats() {
  try {
    const groups = await state.sock.groupFetchAllParticipating()
    for (const [jid, meta] of Object.entries(groups)) {
      state.groups.set(jid, meta)
      if (!state.chats.has(jid)) state.chats.set(jid, { id: jid, name: meta.subject })
    }
  } catch {}
  const chats = [...state.chats.values()].sort(
    (a, b) => toTimestampMs(b.conversationTimestamp) - toTimestampMs(a.conversationTimestamp),
  )
  return chats.map((chat) => {
    const buffered = state.messages.get(chat.id ?? '')
    const last = buffered && buffered.length > 0 ? summarizeMessage(buffered[buffered.length - 1]) : undefined
    const meta = state.groups.get(chat.id)
    const members = meta?.participants?.map((participant) => ({ id: participant.id }))
    return summarizeChat(chat, last, members)
  })
}

const handlers = {
  'auth.status': async (params) => {
    const authDir = params.auth_dir
    if (!authDir || !existsSync(join(authDir, 'creds.json'))) {
      return { authenticated: false, account: null }
    }
    let creds
    try {
      creds = JSON.parse(await readFile(join(authDir, 'creds.json'), 'utf8'))
    } catch {
      return { authenticated: false, account: null }
    }
    const id = creds?.me?.id ?? ''
    if (!id) return { authenticated: false, account: null }
    const phone = id.includes(':') ? id.split(':')[0] : id.split('@')[0]
    return {
      authenticated: true,
      account: { id, name: creds.me?.name ?? null, phone_number: phone || null },
    }
  },

  'auth.login': async (params) => {
    const authDir = params.auth_dir
    if (!authDir) throw Object.assign(new Error('auth_dir is required'), { code: -32002 })
    if (!existsSync(join(authDir, 'creds.json'))) {
      await mkdir(authDir, { recursive: true })
      return {
        authenticated: false,
        account_id: null,
        next_action: 'scan_qr',
        message: 'Link this device by scanning the QR code the sidecar prints, then retry.',
      }
    }
    const status = await handlers['auth.status'](params)
    return {
      authenticated: status.authenticated,
      account_id: status.account?.id ?? null,
      next_action: null,
      message: null,
    }
  },

  'chats.list': async (params) => {
    await connect(params.auth_dir)
    let chats = await collectChats()
    if (typeof params.chat_id === 'string' && params.chat_id) {
      const target = resolveJid(params.chat_id)
      chats = chats.filter((chat) => chat.id === target || chat.id === params.chat_id)
    }
    if (typeof params.search === 'string' && params.search) {
      const needle = params.search.toLowerCase()
      chats = chats.filter(
        (chat) => chat.name.toLowerCase().includes(needle) || chat.id.toLowerCase().includes(needle),
      )
    }
    if (typeof params.limit === 'number') chats = chats.slice(0, params.limit)
    return chats
  },

  'messages.list': async (params) => {
    await connect(params.auth_dir)
    const jid = resolveJid(params.chat_id)
    const limit = typeof params.limit === 'number' && params.limit > 0 ? params.limit : 25
    const buffered = state.messages.get(jid) ?? []
    let sorted = [...buffered].sort((a, b) => toTimestampMs(a.messageTimestamp) - toTimestampMs(b.messageTimestamp))
    if (typeof params.search === 'string' && params.search) {
      const needle = params.search.toLowerCase()
      sorted = sorted.filter((message) =>
        (extractMessageText(messageContent(message)) ?? '').toLowerCase().includes(needle),
      )
    }
    return sorted.slice(-limit).map(summarizeMessage)
  },

  'message.send': async (params) => {
    const sock = await connect(params.auth_dir)
    const jid = resolveJid(params.chat_id)
    const result = await sock.sendMessage(jid, { text: String(params.text ?? '') })
    if (!result) throw Object.assign(new Error('failed to send the message'), { code: -32003 })
    if (!state.chats.has(jid)) {
      const contact = state.contacts.get(jid)
      state.chats.set(jid, { id: jid, name: contact?.name ?? jid })
    }
    return summarizeMessage(result)
  },
}

const reader = createInterface({ input: process.stdin })
reader.on('line', async (line) => {
  const trimmed = line.trim()
  if (!trimmed) return
  let request
  try {
    request = JSON.parse(trimmed)
  } catch {
    return
  }
  if (!request || typeof request !== 'object' || request.id === undefined) return
  const handler = handlers[request.method]
  if (!handler) {
    write({ jsonrpc: '2.0', id: request.id, error: { code: -32601, message: `unknown method ${request.method}` } })
    return
  }
  try {
    const result = await handler(request.params ?? {})
    write({ jsonrpc: '2.0', id: request.id, result })
  } catch (error) {
    write({
      jsonrpc: '2.0',
      id: request.id,
      error: { code: Number(error?.code) || -32000, message: String(error?.message ?? error) },
    })
  }
})
