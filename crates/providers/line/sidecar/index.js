#!/usr/bin/env node
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { createInterface } from 'node:readline'

import { loginWithAuthToken, loginWithPassword } from '@jsr/evex__linejs'
import { FileStorage } from '@jsr/evex__linejs/storage'

const MAX_MESSAGE_ID = 9223372036854775807n
const DEFAULT_DEVICE = 'ANDROIDSECONDARY'

let client = null

function configDir() {
  return process.env.HORON_MESSENGER_CONFIG_DIR || join(process.env.HOME || '.', '.horon-messenger')
}

function storagePath(accountId) {
  const dir = join(configDir(), 'line-storage')
  mkdirSync(dir, { recursive: true })
  return join(dir, `${accountId || 'default'}.json`)
}

function rpcError(code, message) {
  const error = new Error(message)
  error.rpcCode = code
  return error
}

function classify(error) {
  const message = error instanceof Error ? error.message : String(error)
  if (error && error.rpcCode) return error.rpcCode
  if (/e2ee|letter sealing|retry_encrypt|cannot send using plain mode/i.test(message)) return 'E2EE_REQUIRED'
  if (/auth|token|login|unauthorized|invalid/i.test(message)) return 'AUTH_FAILED'
  if (/not found|no such/i.test(message)) return 'NOT_FOUND'
  return 'INTERNAL'
}

function mapChatType(rawType) {
  if (rawType === 'GROUP' || rawType === 0) return 'group'
  if (rawType === 'ROOM' || rawType === 1) return 'room'
  if (rawType === 'PEER' || rawType === 2) return 'user'
  return 'square'
}

async function authLogin(params) {
  const device = params.device || DEFAULT_DEVICE
  const storage = new FileStorage(storagePath(params.account_id))
  if (params.token) {
    client = await loginWithAuthToken(params.token, { device, storage })
  } else if (params.email && params.password) {
    client = await loginWithPassword({ email: params.email, password: params.password, e2ee: false }, { device, storage })
  } else {
    throw rpcError('AUTH_REQUIRED', 'login needs a token, or an email and password')
  }
  const profile = await client.base.talk.getProfile()
  return {
    authenticated: true,
    account_id: profile.mid,
    display_name: profile.displayName,
    device,
    auth_token: client.authToken,
  }
}

async function whoami() {
  const profile = await client.base.talk.getProfile()
  return {
    mid: profile.mid,
    display_name: profile.displayName,
    status_message: profile.statusMessage || undefined,
    picture_url: profile.picturePath ? `https://profile.line-scdn.net${profile.picturePath}` : undefined,
  }
}

async function listChats(limit) {
  const boxes = await client.base.talk.getMessageBoxes({
    messageBoxListRequest: { messageBoxCountLimit: limit, lastMessagesPerMessageBoxCount: 0 },
    syncReason: 'INTERNAL',
  })
  const messageBoxes = boxes?.messageBoxes ?? []
  const seen = new Set()
  const userMids = []
  const groupMids = []
  for (const box of messageBoxes) {
    if (seen.has(box.id)) continue
    seen.add(box.id)
    if (box.midType === 'USER') userMids.push(box.id)
    else groupMids.push(box.id)
  }

  const nameMap = new Map()
  if (userMids.length > 0) {
    try {
      const contacts = await client.base.talk.getContacts({ mids: userMids })
      for (const contact of contacts ?? []) nameMap.set(contact.mid, { name: contact.displayName, type: 'user' })
    } catch (error) {
      process.stderr.write(`[line] could not resolve user names: ${error}\n`)
    }
  }
  if (groupMids.length > 0) {
    try {
      const { chats } = await client.base.talk.getChats({ chatMids: groupMids })
      for (const chat of chats ?? []) {
        const memberMids = chat.extra?.groupExtra?.memberMids
        nameMap.set(chat.chatMid, {
          name: chat.chatName ?? chat.chatMid,
          type: mapChatType(chat.type),
          memberCount: memberMids ? Object.keys(memberMids).length : undefined,
        })
      }
    } catch (error) {
      process.stderr.write(`[line] could not resolve group names: ${error}\n`)
    }
  }

  return {
    chats: messageBoxes.map((box) => {
      const info = nameMap.get(box.id)
      return {
        chat_id: box.id,
        type: info?.type ?? (box.midType === 'USER' ? 'user' : 'group'),
        display_name: info?.name ?? box.id,
        member_count: info?.memberCount,
      }
    }),
  }
}

async function listMessages(chatId, count) {
  const serverTime = await client.base.talk.getServerTime()
  const raw = await client.base.talk.getPreviousMessagesV2WithRequest({
    request: {
      messageBoxId: chatId,
      endMessageId: { deliveredTime: BigInt(serverTime), messageId: MAX_MESSAGE_ID },
      messagesCount: count,
    },
  })
  return {
    messages: (raw ?? []).map((message) => ({
      message_id: String(message.id),
      chat_id: chatId,
      author_id: String(message.from ?? ''),
      text: message.text || null,
      content_type: String(message.contentType ?? 'NONE'),
      sent_at: new Date(Number(message.createdTime)).toISOString(),
    })),
  }
}

async function listMembers(chatId) {
  const { chats } = await client.base.talk.getChats({ chatMids: [chatId] })
  const memberMids = chats?.[0]?.extra?.groupExtra?.memberMids
  if (!memberMids) return { members: [] }
  const mids = Object.keys(memberMids)
  const contacts = await client.base.talk.getContacts({ mids }).catch(() => [])
  const names = new Map((contacts ?? []).map((contact) => [contact.mid, contact.displayName]))
  return { members: mids.map((mid) => ({ mid, display_name: names.get(mid) ?? undefined })) }
}

async function sendMessage(chatId, text) {
  const sent = await client.base.talk.sendMessage({ to: chatId, text, e2ee: false })
  return {
    success: true,
    chat_id: chatId,
    message_id: String(sent.id),
    sent_at: new Date(Number(sent.createdTime)).toISOString(),
  }
}

const handlers = {
  'auth.login': (params) => authLogin(params),
  'auth.status': async () => ({ authenticated: client !== null }),
  'auth.logout': async () => {
    client = null
    return {}
  },
  whoami: () => whoami(),
  'chats.list': (params) => listChats(params.limit ?? 50),
  'messages.list': (params) => listMessages(params.chat_id, params.count ?? 50),
  'members.list': (params) => listMembers(params.chat_id),
  'message.send': (params) => sendMessage(params.chat_id, params.text),
}

function emit(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`)
}

async function handle(request) {
  const handler = handlers[request.method]
  if (!handler) {
    emit({ jsonrpc: '2.0', id: request.id, error: { code: -32601, message: 'Method not found' } })
    return
  }
  try {
    const result = await handler(request.params ?? {})
    emit({ jsonrpc: '2.0', id: request.id, result })
  } catch (error) {
    emit({
      jsonrpc: '2.0',
      id: request.id,
      error: { code: -32000, message: error instanceof Error ? error.message : String(error), data: classify(error) },
    })
  }
}

const lines = createInterface({ input: process.stdin })
lines.on('line', (line) => {
  const trimmed = line.trim()
  if (!trimmed) return
  let request
  try {
    request = JSON.parse(trimmed)
  } catch {
    return
  }
  handle(request)
})
