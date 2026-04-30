export interface ServerInfo {
  id: string
  bind_address: string
  port: number
  state: string
  station_count: number
}

export interface StationInfo {
  common_address: number
  name: string
  point_count: number
}

export interface DataPointInfo {
  ioa: number
  asdu_type: string
  category: string
  name: string
  comment: string
  value: string
  quality_iv: boolean
  timestamp: string | null
}

export interface LogEntry {
  timestamp: string
  direction: string
  frame_label: { [key: string]: string } | string
  detail: string
  raw_bytes: number[] | null
  detail_event?: { kind: string; payload: Record<string, unknown> } | null
}

// ---------------------------------------------------------------------------
// Frame parser (parse_frame_full Tauri command result)
// ---------------------------------------------------------------------------

export interface ParsedQuality {
  ov: boolean
  bl: boolean
  sb: boolean
  nt: boolean
  iv: boolean
}

export interface ParsedTimestamp {
  year: number
  month: number
  day: number
  day_of_week: number
  hour: number
  minute: number
  millisecond: number
  invalid: boolean
  summer_time: boolean
}

export interface ParsedObject {
  ioa: number
  value: { type: string; [k: string]: unknown } | null
  quality: ParsedQuality | null
  timestamp: ParsedTimestamp | null
  raw_hex: string
}

export interface ParsedAsdu {
  type_id: number
  type_name: string
  sq: boolean
  num_objects: number
  cot: number
  cot_name: string
  negative: boolean
  test: boolean
  originator: number
  common_address: number
  objects: ParsedObject[]
}

export type ParsedApci =
  | { frame_type: 'i'; send_seq: number; recv_seq: number }
  | { frame_type: 's'; recv_seq: number }
  | { frame_type: 'u'; kind: string; name: string }

export interface ParsedFrame {
  raw_hex: string
  length: number
  start_byte: number
  apdu_length: number
  control_field: [number, number, number, number]
  apci: ParsedApci
  asdu: ParsedAsdu | null
  warnings: string[]
}
