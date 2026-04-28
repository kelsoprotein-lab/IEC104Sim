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
    add: string
    loading: string
  }
  toolbar: {
    newServer: string
    start: string
    stop: string
    addStation: string
    randomMutation: string
    stopMutation: string
    cyclicSend: string
    stopCyclic: string
    mutationInterval: string
    sendInterval: string
    appTitle: string
    about: string
    titleNewServer: string
    titleStartServer: string
    titleStopServer: string
    titleAddStation: string
    titleRandomMutation: string
    titleCyclicSend: string
  }
  newServer: {
    title: string
    portLabel: string
    initMode: string
    initZero: string
    initRandom: string
    enableTls: string
    serverCert: string
    serverKey: string
    caFile: string
    requireClientCert: string
  }
  prompt: {
    inputCommonAddress: string
    inputStationName: string
    defaultStationName: string
  }
  station: {
    defaultName: string
  }
  tree: {
    title: string
    noServers: string
    ctxStartServer: string
    ctxStopServer: string
    ctxDeleteServer: string
    ctxDeleteStation: string
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
  asduType: {
    sp: string
    dp: string
    st: string
    bo: string
    me_na: string
    me_nb: string
    me_nc: string
    it: string
  }
  table: {
    allPoints: string
    countSuffix: string
    searchPlaceholder: string
    addPointTitle: string
    batchAdd: string
    chooseStation: string
    noPoints: string
    asduTypeCol: string
    nameCol: string
    valueCol: string
    qualityCol: string
    timestampCol: string
    deletePoint: string
  }
  pointModal: {
    title: string
    ioaLabel: string
    ioaPlaceholder: string
    asduTypeLabel: string
    nameLabel: string
    namePlaceholder: string
    commentLabel: string
    commentPlaceholder: string
    saving: string
    add: string
  }
  batchModal: {
    title: string
    startIoa: string
    count: string
    asduTypeLabel: string
    namePrefix: string
    namePrefixPlaceholder: string
    countWarn: string
    rangeHint: string
    saving: string
    add: string
    failedPrefix: string
  }
  valuePanel: {
    title: string
    selectPointHint: string
    sectionInfo: string
    asduType: string
    category: string
    name: string
    comment: string
    sectionCurrent: string
    value: string
    quality: string
    qualityValid: string
    qualityInvalid: string
    timestamp: string
    sectionWrite: string
    valuePlaceholder: string
    write: string
    sectionMultiSelect: string
    countLabel: string
  }
  log: {
    title: string
    refresh: string
    clear: string
    export: string
    loading: string
    chooseServer: string
    noLogs: string
    timeCol: string
    directionCol: string
    frameCol: string
    detailCol: string
    rawCol: string
    titleRefresh: string
    titleClear: string
    titleExport: string
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
    invalidPort: string
    invalidCa: string
    invalidIoa: string
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
    add: '添加',
    loading: '加载中...',
  },
  toolbar: {
    newServer: '新建服务器',
    start: '启动',
    stop: '停止',
    addStation: '添加站',
    randomMutation: '随机变化',
    stopMutation: '停止变化',
    cyclicSend: '周期发送',
    stopCyclic: '停止周期',
    mutationInterval: '变化间隔 (ms)',
    sendInterval: '发送间隔 (ms)',
    appTitle: 'IEC 104 Slave',
    about: '关于',
    titleNewServer: '新建服务器',
    titleStartServer: '启动服务器',
    titleStopServer: '停止服务器',
    titleAddStation: '添加站',
    titleRandomMutation: '随机变化',
    titleCyclicSend: '周期发送',
  },
  newServer: {
    title: '新建服务器',
    portLabel: '端口号',
    initMode: '初始值',
    initZero: '全零',
    initRandom: '随机',
    enableTls: '启用 TLS',
    serverCert: '服务器证书文件 (PEM)',
    serverKey: '服务器密钥文件 (PEM)',
    caFile: 'CA 证书文件 (PEM, 可选)',
    requireClientCert: '要求客户端证书 (mTLS)',
  },
  prompt: {
    inputCommonAddress: '输入公共地址 (CA)',
    inputStationName: '输入站名',
    defaultStationName: '站 {ca}',
  },
  station: {
    defaultName: '站 {ca}',
  },
  tree: {
    title: '服务器',
    noServers: '暂无服务器',
    ctxStartServer: '启动服务器',
    ctxStopServer: '停止服务器',
    ctxDeleteServer: '删除服务器',
    ctxDeleteStation: '删除站',
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
  asduType: {
    sp: 'M_SP_NA_1 - 单点信息',
    dp: 'M_DP_NA_1 - 双点信息',
    st: 'M_ST_NA_1 - 步位置信息',
    bo: 'M_BO_NA_1 - 位串',
    me_na: 'M_ME_NA_1 - 归一化测量值',
    me_nb: 'M_ME_NB_1 - 标度化测量值',
    me_nc: 'M_ME_NC_1 - 浮点测量值',
    it: 'M_IT_NA_1 - 累计量',
  },
  table: {
    allPoints: '全部数据点',
    countSuffix: '个数据点',
    searchPlaceholder: '搜索 IOA / 名称...',
    addPointTitle: '添加数据点',
    batchAdd: '批量',
    chooseStation: '请在左侧树形导航中选择一个站',
    noPoints: '暂无数据点',
    asduTypeCol: 'ASDU 类型',
    nameCol: '名称',
    valueCol: '值',
    qualityCol: '品质',
    timestampCol: '时间戳',
    deletePoint: '删除数据点',
  },
  pointModal: {
    title: '添加数据点',
    ioaLabel: 'IOA (信息对象地址)',
    ioaPlaceholder: '例如: 100',
    asduTypeLabel: 'ASDU 类型',
    nameLabel: '名称 (可选)',
    namePlaceholder: '可留空',
    commentLabel: '备注 (可选)',
    commentPlaceholder: '可留空',
    saving: '添加中...',
    add: '确认',
  },
  batchModal: {
    title: '批量添加数据点',
    startIoa: '起始 IOA',
    count: '数量',
    asduTypeLabel: 'ASDU 类型',
    namePrefix: '名称前缀（可选）',
    namePrefixPlaceholder: '如 SP → SP_0, SP_1, ...',
    countWarn: '范围过大（最多 100000）',
    rangeHint: 'IOA 范围：{startIoa} ~ {endIoa}，共将添加 {count} 个数据点',
    saving: '添加中...',
    add: '确认',
    failedPrefix: '批量添加失败：{err}',
  },
  valuePanel: {
    title: '数据点详情',
    selectPointHint: '选择一个数据点查看详情',
    sectionInfo: '基本信息',
    asduType: 'ASDU 类型',
    category: '分类',
    name: '名称',
    comment: '备注',
    sectionCurrent: '当前值',
    value: '值',
    quality: '品质',
    qualityValid: '正常',
    qualityInvalid: 'IV (无效)',
    timestamp: '时间戳',
    sectionWrite: '写入值',
    valuePlaceholder: '输入新值',
    write: '写入',
    sectionMultiSelect: '批量选中',
    countLabel: '数量',
  },
  log: {
    title: '通信日志',
    refresh: '刷新',
    clear: '清除',
    export: '导出CSV',
    loading: '加载中...',
    chooseServer: '请先选择一个服务器',
    noLogs: '暂无日志',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    detailCol: '详情',
    rawCol: '原始数据',
    titleRefresh: '刷新',
    titleClear: '清除',
    titleExport: '导出CSV',
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
    invalidPort: '请输入有效的端口号 (1-65535)',
    invalidCa: '请输入有效的公共地址 (1-65534)',
    invalidIoa: '请输入有效的 IOA (>= 0)',
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
