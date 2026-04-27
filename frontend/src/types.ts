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
