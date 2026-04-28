export type DictShape = {
  common: {
    confirm: string
    cancel: string
    ok: string
    close: string
    save: string
    refresh: string
    clear: string
    export: string
    delete: string
  }
  toolbar: {
    newConnection: string
    connect: string
    disconnect: string
    delete: string
    sendGI: string
    clockSync: string
    counterRead: string
    appTitle: string
    about: string
  }
  newConn: {
    title: string
    targetAddress: string
    port: string
    commonAddress: string
    enableTls: string
    tlsVersion: string
    tlsAuto: string
    tls12: string
    tls13: string
    caFile: string
    certFile: string
    keyFile: string
    acceptInvalidCerts: string
    create: string
  }
  tree: {
    title: string
    noConnections: string
    deleteConnection: string
  }
  category: {
    single_point: string
    double_point: string
    step_position: string
    bitstring: string
    normalized_measured: string
    scaled_measured: string
    float_measured: string
    integrated_totals: string
  }
  table: {
    allData: string
    countSuffix: string
    countOf: string
    chooseConnection: string
    searchPlaceholder: string
    type: string
    value: string
    quality: string
    timestamp: string
    noDataHint: string
    setpoint: string
    copyIoa: string
    copyValue: string
    freeControl: string
  }
  valuePanel: {
    title: string
    selectPointHint: string
    selectedPoint: string
    type: string
    category: string
    value: string
    quality: string
    qualityValid: string
    qualityInvalid: string
    timestamp: string
    timestampNone: string
    quickControl: string
    sendSetpoint: string
    sboLabel: string
    sboTwoStep: string
    sboDirect: string
    notControllable: string
    doubleIntermediate: string
    doubleInvalid: string
  }
  log: {
    title: string
    noConnections: string
    noLogs: string
    timeCol: string
    directionCol: string
    frameCol: string
    detailCol: string
    rawCol: string
    refresh: string
    clear: string
    export: string
    singleCommand: string
    doubleCommand: string
    stepCommand: string
    setpointNormalized: string
    setpointScaled: string
    setpointFloat: string
  }
  control: {
    title: string
    targetCa: string
    ioa: string
    commandType: string
    cmdSingle: string
    cmdDouble: string
    cmdStep: string
    cmdSetNorm: string
    cmdSetScaled: string
    cmdSetFloat: string
    cmdBitstring: string
    optOff: string
    optOn: string
    optIntermediate: string
    optOpen: string
    optClose: string
    optInvalid: string
    optStepDown: string
    optStepUp: string
    valueRangeScaled: string
    valueLabel: string
    valueRangeBitstring: string
    bitstringHex: string
    sboLabel: string
    sboTwoStep: string
    sboDirect: string
    bitstringNoSbo: string
    advancedSummary: string
    qulqlLabel: string
    qulqlIgnored: string
    qulqlSingle: string
    qulqlSetpoint: string
    qulqlBitstring: string
    cotLabel: string
    cot6: string
    cot7: string
    cot8: string
    cot9: string
    cot10: string
    sending: string
    send: string
  }
  about: {
    whatsNew: string
    homepage: string
    homepageLabel: string
    releasesLabel: string
    copiedSuffix: string
  }
  appDialog: {
    cancel: string
    ok: string
    titleAlert: string
    titleConfirm: string
    titlePrompt: string
  }
  errors: {
    connectFailed: string
  }
  update: {
    available: string
    newVersion: string
    changelog: string
    installNow: string
    later: string
    downloading: string
    failedTitle: string
    retry: string
    close: string
  }
  _test: { interp: string }
}

const dict: DictShape = {
  common: {
    confirm: '确认',
    cancel: '取消',
    ok: '确定',
    close: '关闭',
    save: '保存',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    delete: '删除',
  },
  toolbar: {
    newConnection: '新建连接',
    connect: '连接',
    disconnect: '断开',
    delete: '删除',
    sendGI: '总召唤',
    clockSync: '时钟同步',
    counterRead: '累计量召唤',
    appTitle: 'IEC104 Master',
    about: '关于',
  },
  newConn: {
    title: '新建连接',
    targetAddress: '目标地址',
    port: '端口',
    commonAddress: '公共地址 (CA)',
    enableTls: '启用 TLS',
    tlsVersion: 'TLS 版本',
    tlsAuto: '自动',
    tls12: '仅 TLS 1.2',
    tls13: '仅 TLS 1.3',
    caFile: 'CA 证书路径',
    certFile: '客户端证书路径',
    keyFile: '客户端密钥路径',
    acceptInvalidCerts: '接受无效证书（测试用）',
    create: '创建',
  },
  tree: {
    title: '连接列表',
    noConnections: '暂无连接',
    deleteConnection: '删除连接',
  },
  category: {
    single_point: '单点 (SP)',
    double_point: '双点 (DP)',
    step_position: '步位置 (ST)',
    bitstring: '位串 (BO)',
    normalized_measured: '归一化 (ME_NA)',
    scaled_measured: '标度化 (ME_NB)',
    float_measured: '浮点 (ME_NC)',
    integrated_totals: '累计量 (IT)',
  },
  table: {
    allData: '全部数据',
    countSuffix: '个',
    countOf: '/',
    chooseConnection: '选择一个连接查看数据',
    searchPlaceholder: '搜索 IOA / 类型...',
    type: '类型',
    value: '值',
    quality: '品质',
    timestamp: '时间戳',
    noDataHint: '暂无数据，请先发送总召唤',
    setpoint: '设定值...',
    copyIoa: '复制 IOA',
    copyValue: '复制值',
    freeControl: '自由控制...',
  },
  valuePanel: {
    title: '数据详情',
    selectPointHint: '选择数据点查看详情',
    selectedPoint: '选中数据点',
    type: '类型',
    category: '分类',
    value: '值',
    quality: '品质',
    qualityValid: 'OK (有效)',
    qualityInvalid: 'IV (无效)',
    timestamp: '时间戳',
    timestampNone: '无',
    quickControl: '快捷控制',
    sendSetpoint: '发送设定值',
    sboLabel: '选择-执行 (SbO)',
    sboTwoStep: '自动两步',
    sboDirect: '直接执行',
    notControllable: '此类型不支持控制操作',
    doubleIntermediate: '中间',
    doubleInvalid: '不确定',
  },
  log: {
    title: '通信日志',
    noConnections: '暂无连接',
    noLogs: '暂无日志',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    detailCol: '详情',
    rawCol: '原始数据',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    singleCommand: '单点命令 IOA={ioa} val={val}',
    doubleCommand: '双点命令 IOA={ioa} val={val}',
    stepCommand: '步调节命令 IOA={ioa} val={val}',
    setpointNormalized: '归一化设定值 IOA={ioa} val={val}',
    setpointScaled: '标度化设定值 IOA={ioa} val={val}',
    setpointFloat: '浮点设定值 IOA={ioa} val={val}',
  },
  control: {
    title: '发送控制命令',
    targetCa: '目标公共地址 (CA)',
    ioa: 'IOA (信息对象地址)',
    commandType: '命令类型',
    cmdSingle: '单点命令 (C_SC_NA_1)',
    cmdDouble: '双点命令 (C_DC_NA_1)',
    cmdStep: '步调节命令 (C_RC_NA_1)',
    cmdSetNorm: '归一化设定值 (C_SE_NA_1)',
    cmdSetScaled: '标度化设定值 (C_SE_NB_1)',
    cmdSetFloat: '浮点设定值 (C_SE_NC_1)',
    cmdBitstring: '位串命令 (C_BO_NA_1)',
    optOff: '分闸 OFF',
    optOn: '合闸 ON',
    optIntermediate: '中间',
    optOpen: '分',
    optClose: '合',
    optInvalid: '不确定',
    optStepDown: '降',
    optStepUp: '升',
    valueRangeScaled: '值 (-32768 ~ 32767)',
    valueLabel: '值',
    valueRangeBitstring: '值 (32 位无符号)',
    bitstringHex: '十六进制',
    sboLabel: '选择-执行 (SbO)',
    sboTwoStep: '自动两步',
    sboDirect: '直接执行',
    bitstringNoSbo: '位串命令不支持 SbO',
    advancedSummary: '高级参数 (QU/QL/COT)',
    qulqlLabel: 'QU/QL 限定词',
    qulqlIgnored: 'QU/QL (忽略)',
    qulqlSingle: 'QU: 0=无附加定义, 1=短脉冲, 2=长脉冲, 3=持续 (写入命令字节 bit2..6)',
    qulqlSetpoint: 'QL: 0..127 (写入 QOS 低 7 位)',
    qulqlBitstring: '位串命令无 QU/QL,本字段忽略',
    cotLabel: 'COT 传送原因',
    cot6: '6 - 激活',
    cot7: '7 - 激活确认',
    cot8: '8 - 停止激活',
    cot9: '9 - 停止激活确认',
    cot10: '10 - 激活终止',
    sending: '发送中...',
    send: '发送',
  },
  about: {
    whatsNew: '本次更新',
    homepage: '项目主页',
    homepageLabel: '项目主页',
    releasesLabel: '历史版本',
    copiedSuffix: '已复制到剪贴板',
  },
  appDialog: {
    cancel: '取消',
    ok: '确定',
    titleAlert: '提示',
    titleConfirm: '确认',
    titlePrompt: '输入',
  },
  errors: {
    connectFailed: '连接失败: {err}\n将每 {sec} 秒自动重试,点击「断开」可停止。',
  },
  update: {
    available: '检测到新版本',
    newVersion: '新版本 v{version} 可用',
    changelog: '更新说明',
    installNow: '立即更新',
    later: '稍后',
    downloading: '正在下载 {pct}%',
    failedTitle: '更新失败',
    retry: '重试',
    close: '关闭',
  },
  _test: {
    interp: '订单 #{id} 由 {user} 创建',
  },
}

export default dict
