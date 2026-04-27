export interface ConnectionInfo {
  id: string
  target_address: string
  port: number
  common_address: number
  state: string
  use_tls: boolean
}

export interface ReceivedDataPointInfo {
  ioa: number
  asdu_type: string
  category: string
  value: string
  quality_iv: boolean
  timestamp: string | null
  update_seq: number
}

export interface IncrementalDataResponse {
  seq: number
  total_count: number
  points: ReceivedDataPointInfo[]
}

export interface LogEntry {
  timestamp: string
  direction: string
  frame_label: { [key: string]: string } | string
  detail: string
  raw_bytes: number[] | null
  detail_event?: { kind: string; payload: Record<string, unknown> } | null
}

export type CommandType = 'single' | 'double' | 'step' | 'setpoint_normalized' | 'setpoint_scaled' | 'setpoint_float' | 'bitstring'

export interface ControlCommandRequest {
  connection_id: string
  ioa: number
  common_address: number
  command_type: CommandType
  value: string
  select?: boolean
  qualifier?: number
  cot?: number
  bitstring?: number
}

export interface RawApduRequest {
  connection_id: string
  hex_payload: string
}

export interface RawSendResult {
  sent_hex: string
  byte_len: number
  timestamp: string
}

export interface ControlStep {
  action: string
  timestamp: string
}

export interface ControlResult {
  steps: ControlStep[]
  duration_ms: number
}

export type WidgetType = 'toggle' | 'button_group' | 'step_buttons' | 'slider' | 'number_input'

export interface ControlOption {
  label: string
  value: string
}

export interface ControlConfig {
  commandType: CommandType
  label: string
  widget: WidgetType
  options?: ControlOption[]
  min?: number
  max?: number
  step?: number
}

import { useI18n } from './i18n'

export function getControlConfig(category: string): ControlConfig | null {
  const { t } = useI18n()
  switch (category) {
    case '单点 (SP)':
      return {
        commandType: 'single',
        label: t('control.cmdSingle'),
        widget: 'toggle',
        options: [
          { label: t('control.optOff'), value: 'false' },
          { label: t('control.optOn'), value: 'true' },
        ],
      }
    case '双点 (DP)':
      return {
        commandType: 'double',
        label: t('control.cmdDouble'),
        widget: 'button_group',
        options: [
          { label: t('control.optIntermediate'), value: '0' },
          { label: t('control.optOpen'), value: '1' },
          { label: t('control.optClose'), value: '2' },
          { label: t('control.optInvalid'), value: '3' },
        ],
      }
    case '步位置 (ST)':
      return {
        commandType: 'step',
        label: t('control.cmdStep'),
        widget: 'step_buttons',
        options: [
          { label: t('control.optStepDown'), value: '1' },
          { label: t('control.optStepUp'), value: '2' },
        ],
      }
    case '归一化 (ME_NA)':
      return {
        commandType: 'setpoint_normalized',
        label: t('control.cmdSetNorm'),
        widget: 'slider',
        min: -1.0, max: 1.0, step: 0.001,
      }
    case '标度化 (ME_NB)':
      return {
        commandType: 'setpoint_scaled',
        label: t('control.cmdSetScaled'),
        widget: 'number_input',
        min: -32768, max: 32767, step: 1,
      }
    case '浮点 (ME_NC)':
      return {
        commandType: 'setpoint_float',
        label: t('control.cmdSetFloat'),
        widget: 'number_input',
        step: 0.1,
      }
    case '位串 (BO)':
      return {
        commandType: 'bitstring',
        label: t('control.cmdBitstring'),
        widget: 'number_input',
        min: 0, max: 0xFFFFFFFF, step: 1,
      }
    case '累计量 (IT)':
    default:
      return null
  }
}
