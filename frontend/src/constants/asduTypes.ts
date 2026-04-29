// 共享 ASDU 类型清单：用于 BatchAddModal / DataPointModal 等 dropdown。
// `value` 是后端 `parse_asdu_type` 接受的 PascalCase 枚举名；
// `labelKey` 是 i18n 字典里的 key（zh-CN / en-US 在 asduType.* 下定义）。
export interface AsduTypeOption {
  value: string
  labelKey: string
}

export const ASDU_TYPE_OPTIONS: AsduTypeOption[] = [
  { value: 'MSpNa1', labelKey: 'asduType.sp' },
  { value: 'MSpTb1', labelKey: 'asduType.sp_tb' },
  { value: 'MDpNa1', labelKey: 'asduType.dp' },
  { value: 'MDpTb1', labelKey: 'asduType.dp_tb' },
  { value: 'MStNa1', labelKey: 'asduType.st' },
  { value: 'MStTb1', labelKey: 'asduType.st_tb' },
  { value: 'MBoNa1', labelKey: 'asduType.bo' },
  { value: 'MBoTb1', labelKey: 'asduType.bo_tb' },
  { value: 'MMeNa1', labelKey: 'asduType.me_na' },
  { value: 'MMeTd1', labelKey: 'asduType.me_td' },
  { value: 'MMeNb1', labelKey: 'asduType.me_nb' },
  { value: 'MMeTe1', labelKey: 'asduType.me_te' },
  { value: 'MMeNc1', labelKey: 'asduType.me_nc' },
  { value: 'MMeTf1', labelKey: 'asduType.me_tf' },
  { value: 'MItNa1', labelKey: 'asduType.it' },
  { value: 'MItTb1', labelKey: 'asduType.it_tb' },
]
