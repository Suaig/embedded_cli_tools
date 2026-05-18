#!/usr/bin/env bash
#
# emb 自动化集成测试脚本
# 用法：从项目根目录运行
#   bash tests/run_tests.sh
#
# 依赖：target/release/emb（需先 cargo build --release）
# 可选：jq（用于 JSON 验证，缺失时跳过 JSON 结构验证）

set -euo pipefail

# ---------------------------------------------------------------------------
# 配置
# ---------------------------------------------------------------------------
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EMB="$PROJECT_ROOT/target/release/emb"

TP="$PROJECT_ROOT/Test_Project"
WORKSPACE="$TP/MDK-ARM/Project.uvmpw"
BOOT_PROJ="$TP/MDK-ARM/Boot/Test_Boot.uvprojx"
APPLI_PROJ="$TP/MDK-ARM/Appli/Test_Appli.uvprojx"
IOC="$TP/Test.ioc"

PASS=0
FAIL=0
TESTS=()

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# 辅助函数
# ---------------------------------------------------------------------------
check_binary() {
    if [[ ! -f "$EMB" ]]; then
        echo -e "${RED}错误: emb 不存在 ($EMB)${NC}"
        echo "请先运行: cargo build --release"
        exit 1
    fi
}

run_test() {
    local name="$1"
    local expect_exit="${2:-0}"
    local pattern="$3"
    shift 3

    TESTS+=("$name")

    local output
    local exit_code=0

    output=$("$EMB" "$@" 2>&1) || exit_code=$?
    # Normalize CRLF
    output=$(echo "$output" | tr -d '\r')

    local ok=true

    if [[ "$expect_exit" == "nz" ]]; then
        if [[ $exit_code -eq 0 ]]; then
            ok=false
        fi
    else
        if [[ $exit_code -ne "$expect_exit" ]]; then
            ok=false
        fi
    fi

    if [[ -n "$pattern" ]] && ! echo "$output" | grep -q "$pattern"; then
        ok=false
    fi

    if $ok; then
        echo -e "  ${GREEN}PASS${NC}  $name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  $name"
        echo "        exit=$exit_code (期望 ${expect_exit})"
        echo "        输出: $(echo "$output" | head -3)"
        FAIL=$((FAIL + 1))
    fi
}

# 不检查退出码（仅用于信息输出）
info() {
    echo -e "  ${YELLOW}INFO${NC}  $1: $2"
}

# ---------------------------------------------------------------------------
# 开始
# ---------------------------------------------------------------------------
check_binary

echo "========================================"
echo "  emb 自动化集成测试"
echo "  Binary: $EMB"
echo "========================================"
echo ""

# =========================================
# 1. CLI 入口
# =========================================
echo "--- 1. CLI 入口 ---"
run_test "无子命令报错"       nz  "no command specified"
run_test "版本信息"            0   "emb"     --version
run_test "帮助信息"            0   "keil"    --help
run_test "帮助含 ioc"          0   "ioc"     --help
run_test "帮助含 serial"       0   "serial"  --help
run_test "帮助含 debug"        0   "debug"   --help
run_test "--ai 和 --json 互斥"  nz  "mutually exclusive"  --ai --json keil info "$APPLI_PROJ"
echo ""

# =========================================
# 2. 错误处理
# =========================================
echo "--- 2. 错误处理 ---"
run_test "不存在的文件"                        nz  ""          keil info nonexistent.uvprojx
run_test "不支持的文件类型"                     nz  "unsupported file type"  keil info not_project.txt
run_test "不存在的 Target"                     nz  "target.*not found"      keil info "$APPLI_PROJ" -t "NoTarget"
run_test "不存在的 workspace 子项目"            nz  "project.*not found"    keil info "$WORKSPACE" -t "Test_Boot" -p "NoProj.uvprojx"
run_test "IOC 不存在的 prefix"                 nz  "No entries found matching prefix"  ioc get "$IOC" NONEXIST
run_test "Config 非法 key"                     nz  "unknown config key"    config set bad_key value
echo ""

# =========================================
# 3. Keil — Info
# =========================================
echo "--- 3. Keil Info ---"
run_test "Workspace 概览"        0  "Test_Boot.uvprojx"   keil info "$WORKSPACE"
run_test "Workspace 概览含 Appli" 0  "Test_Appli.uvprojx"  keil info "$WORKSPACE"
run_test "Boot 单项目概览"        0  "Test_Boot"           keil info "$BOOT_PROJ"
run_test "Boot Target 详情"       0  "STM32H7R7I8Kx"      --ai keil info "$BOOT_PROJ" -t "Test_Boot"
run_test "Boot Target AC6"        0  "AC6:yes"             --ai keil info "$BOOT_PROJ" -t "Test_Boot"
run_test "Appli Target 详情"      0  "Test_Appli"          --ai keil info "$APPLI_PROJ" -t "Test_Appli"
run_test "Workspace + Target 自动选第一个"  0  "Test_Boot"  --ai keil info "$WORKSPACE" -t "Test_Boot"
run_test "Workspace + -p + -t"    0  "Test_Appli"          --ai keil info "$WORKSPACE" -t "Test_Appli" -p "Test_Appli.uvprojx"
echo ""

# =========================================
# 4. Keil — Config
# =========================================
echo "--- 4. Keil Config ---"
run_test "Config 全部"             0  "device.name:STM32H7R7I8Kx"  --ai keil config "$APPLI_PROJ" -t "Test_Appli"
run_test "Config 含 AC6"           0  "ccompiler.ac6:AC6"          --ai keil config "$APPLI_PROJ" -t "Test_Appli"
run_test "Config 含 optim 选项"    0  "\[0=default"                 --ai keil config "$APPLI_PROJ" -t "Test_Appli"
run_test "Config ccompiler 过滤"   0  "ccompiler.optim"             --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler
run_test "Config ccompiler 无 device"  0  ""                       --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler

# 验证 ccompiler 过滤不含 device
do_test_no_cross_contamination() {
    local out
    out=$("$EMB" --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler 2>&1 | tr -d '\r')
    if echo "$out" | grep -q "^device\."; then
        echo -e "  ${RED}FAIL${NC}  Config ccompiler 过滤不该含 device.*"
        FAIL=$((FAIL + 1))
    else
        echo -e "  ${GREEN}PASS${NC}  Config ccompiler 过滤器不泄露其他分类"
        PASS=$((PASS + 1))
    fi
}
do_test_no_cross_contamination

run_test "Config workspace + -p"   0  "ccompiler.optim"  --ai keil config "$WORKSPACE" -t "Test_Boot" -p "Test_Boot.uvprojx" ccompiler
echo ""

# =========================================
# 5. Keil — Config 闭环修改
# =========================================
echo "--- 5. Keil Config 闭环修改 (自动还原) ---"

# 备份
cp "$APPLI_PROJ" "$APPLI_PROJ.testbak"

# 修改 optim
out=$("$EMB" keil config-set "$APPLI_PROJ" -t "Test_Appli" ccompiler.optim 2 2>&1 || true)
if echo "$out" | grep -q "ok"; then
    out2=$("$EMB" --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler 2>&1 | tr -d '\r')
    if echo "$out2" | grep -q "ccompiler.optim:2 (O1)"; then
        echo -e "  ${GREEN}PASS${NC}  Config-set optim=2 生效"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  Config-set optim=2 未生效"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "  ${RED}FAIL${NC}  Config-set optim=2 返回异常: $out"
    FAIL=$((FAIL + 1))
fi

# 还原 optim
"$EMB" keil config-set "$APPLI_PROJ" -t "Test_Appli" ccompiler.optim 4 >/dev/null 2>&1

# 修改 bool
out=$("$EMB" keil config-set "$APPLI_PROJ" -t "Test_Appli" ccompiler.c99 no 2>&1 || true)
if echo "$out" | grep -q "ok"; then
    out2=$("$EMB" --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler 2>&1 | tr -d '\r')
    if echo "$out2" | grep -q "ccompiler.c99:no"; then
        echo -e "  ${GREEN}PASS${NC}  Config-set c99=no 生效"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  Config-set c99=no 未生效"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "  ${RED}FAIL${NC}  Config-set c99=no 返回异常"
    FAIL=$((FAIL + 1))
fi

# 还原 c99
"$EMB" keil config-set "$APPLI_PROJ" -t "Test_Appli" ccompiler.c99 yes >/dev/null 2>&1

# 非法 bool 值
run_test "Config-set 非法 bool"  nz  ""  keil config-set "$APPLI_PROJ" -t "Test_Appli" output.hex maybe

# 未知 key
run_test "Config-set 未知 key"   nz  ""  keil config-set "$APPLI_PROJ" -t "Test_Appli" unknown.key val

# 还原备份
mv "$APPLI_PROJ.testbak" "$APPLI_PROJ"
echo ""

# =========================================
# 6. Keil — Defines
# =========================================
echo "--- 6. Keil Defines (自动还原) ---"
run_test "Defines 列表"          0  "USE_HAL_DRIVER"  --ai keil defines "$APPLI_PROJ" -t "Test_Appli"
run_test "Defines 含 STM32H7R7xx" 0  "STM32H7R7xx"    --ai keil defines "$APPLI_PROJ" -t "Test_Appli"

# 备份
cp "$APPLI_PROJ" "$APPLI_PROJ.testbak"

"$EMB" keil defines-add "$APPLI_PROJ" -t "Test_Appli" TEST_MACRO >/dev/null 2>&1
out=$("$EMB" --ai keil defines "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r')
if echo "$out" | grep -q "TEST_MACRO"; then
    echo -e "  ${GREEN}PASS${NC}  Defines-add 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Defines-add 未生效"
    FAIL=$((FAIL + 1))
fi

"$EMB" keil defines-remove "$APPLI_PROJ" -t "Test_Appli" TEST_MACRO >/dev/null 2>&1
out=$("$EMB" --ai keil defines "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r')
if echo "$out" | grep -qv "TEST_MACRO"; then
    echo -e "  ${GREEN}PASS${NC}  Defines-remove 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Defines-remove 未生效"
    FAIL=$((FAIL + 1))
fi

mv "$APPLI_PROJ.testbak" "$APPLI_PROJ"
echo ""

# =========================================
# 7. Keil — Includes
# =========================================
echo "--- 7. Keil Includes (自动还原) ---"
run_test "Includes 列表"  0  "Drivers"  --ai keil includes "$APPLI_PROJ" -t "Test_Appli"

cp "$APPLI_PROJ" "$APPLI_PROJ.testbak"

"$EMB" keil includes-add "$APPLI_PROJ" -t "Test_Appli" "./TestInc" >/dev/null 2>&1
out=$("$EMB" --ai keil includes "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r')
if echo "$out" | grep -q "TestInc"; then
    echo -e "  ${GREEN}PASS${NC}  Includes-add 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Includes-add 未生效"
    FAIL=$((FAIL + 1))
fi

"$EMB" keil includes-remove "$APPLI_PROJ" -t "Test_Appli" "./TestInc" >/dev/null 2>&1
out=$("$EMB" --ai keil includes "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r')
if echo "$out" | grep -qv "TestInc"; then
    echo -e "  ${GREEN}PASS${NC}  Includes-remove 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Includes-remove 未生效"
    FAIL=$((FAIL + 1))
fi

mv "$APPLI_PROJ.testbak" "$APPLI_PROJ"
echo ""

# =========================================
# 8. Keil — Groups & Files
# =========================================
echo "--- 8. Keil Groups & Files ---"
run_test "Groups 列表"            0  "Application"  keil groups "$APPLI_PROJ" -t "Test_Appli"
run_test "Groups 含 Drivers"      0  "Drivers"       keil groups "$APPLI_PROJ" -t "Test_Appli"
run_test "Files 列表"             0  "main.c"        keil files "$APPLI_PROJ" -t "Test_Appli"
run_test "Files 含 stm32h7rsxx"   0  "stm32h7rsxx_it"  keil files "$APPLI_PROJ" -t "Test_Appli"
echo ""

# =========================================
# 9. IOC 模块
# =========================================
echo "--- 9. IOC ---"
run_test "IOC info 含 Mcu"        0  "Mcu"               ioc info "$IOC"
run_test "IOC info 含 RCC"        0  "RCC"               ioc info "$IOC"
run_test "IOC info 含 NVIC1"      0  "NVIC1"             ioc info "$IOC"
run_test "IOC info 含 ProjectManager" 0  "ProjectManager"  ioc info "$IOC"
run_test "IOC get RCC"            0  "RCC.SYSCLKFreq_VALUE"  --ai ioc get "$IOC" RCC
run_test "IOC get ProjectManager" 0  "ProjectManager.HeapSize"  ioc get "$IOC" ProjectManager
run_test "IOC 精确 key"           0  "STM32H7R7I8Kx"     ioc get "$IOC" Mcu.Name

# IOC 编辑（备份还原）
cp "$IOC" "$IOC.testbak"

"$EMB" ioc set "$IOC" TEST.Key 12345 >/dev/null 2>&1
out=$("$EMB" ioc get "$IOC" TEST 2>&1 | tr -d '\r')
if echo "$out" | grep -q "12345"; then
    echo -e "  ${GREEN}PASS${NC}  IOC set 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  IOC set 未生效"
    FAIL=$((FAIL + 1))
fi

"$EMB" ioc rm "$IOC" TEST.Key >/dev/null 2>&1
out=$("$EMB" ioc get "$IOC" TEST 2>&1 || true)
if echo "$out" | grep -q "No entries found"; then
    echo -e "  ${GREEN}PASS${NC}  IOC rm 生效"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  IOC rm 未生效"
    FAIL=$((FAIL + 1))
fi

mv "$IOC.testbak" "$IOC"
echo ""

# =========================================
# 10. Serial 模块
# =========================================
echo "--- 10. Serial ---"
out=$("$EMB" serial scan 2>&1 || true)
if echo "$out" | grep -qE "(Port|No serial ports)"; then
    echo -e "  ${GREEN}PASS${NC}  Serial scan"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Serial scan 输出异常"
    FAIL=$((FAIL + 1))
fi

run_test "Serial send 不存在端口"  nz  ""  serial send COM99 "test"
echo ""

# =========================================
# 11. Config 模块
# =========================================
echo "--- 11. Config ---"
run_test "Config list"  0  "keil_path"  config list
echo ""

# =========================================
# 12. Debug 模块
# =========================================
echo "--- 12. Debug ---"
run_test "OpenOCD stub"  0  "not yet implemented"  debug openocd -f test.cfg
run_test "Keil debug stub"  0  "not yet implemented"  debug keil test.uvprojx
echo ""

# =========================================
# 13. 输出格式
# =========================================
echo "--- 13. 输出格式 ---"

out=$("$EMB" --json keil info "$APPLI_PROJ" 2>&1 | tr -d '\r')
if echo "$out" | grep -q '"headers"' && echo "$out" | grep -q '"rows"'; then
    echo -e "  ${GREEN}PASS${NC}  JSON Keil info 含 headers/rows"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  JSON Keil info 格式异常"
    FAIL=$((FAIL + 1))
fi

out=$("$EMB" --json ioc get "$IOC" Mcu.Name 2>&1 | tr -d '\r')
if echo "$out" | grep -q "STM32H7R7I8Kx"; then
    echo -e "  ${GREEN}PASS${NC}  JSON IOC get"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  JSON IOC get 格式异常"
    FAIL=$((FAIL + 1))
fi

# jq 验证（可选）
if command -v jq &>/dev/null; then
    if "$EMB" --json keil info "$APPLI_PROJ" 2>&1 | jq . >/dev/null 2>&1; then
        echo -e "  ${GREEN}PASS${NC}  JSON 通过 jq 验证"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  JSON jq 验证失败"
        FAIL=$((FAIL + 1))
    fi
else
    info "jq 未安装，跳过 JSON 结构验证" ""
fi

out=$("$EMB" --ai keil config "$APPLI_PROJ" -t "Test_Appli" ccompiler 2>&1 | head -3 | tr -d '\r')
if echo "$out" | grep -qE "(ccompiler\.[a-z_]+:.*\[.*\])"; then
    echo -e "  ${GREEN}PASS${NC}  AI 模式 config 含可选值范围"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  AI 模式 config 格式异常"
    FAIL=$((FAIL + 1))
fi

out=$("$EMB" keil files "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r')
if echo "$out" | grep -qE "^(┌|├|└|│)"; then
    echo -e "  ${GREEN}PASS${NC}  Human 模式含 comfy-table 框线"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Human 模式格式异常"
    FAIL=$((FAIL + 1))
fi
echo ""

# =========================================
# 14. Workspace 一致性
# =========================================
echo "--- 14. Workspace 一致性 ---"

out_direct=$("$EMB" --ai keil info "$BOOT_PROJ" -t "Test_Boot" 2>&1 | tr -d '\r' | grep "Target:" | head -1)
out_ws=$("$EMB" --ai keil info "$WORKSPACE" -t "Test_Boot" 2>&1 | tr -d '\r' | grep "Target:" | head -1)
if [[ "$out_direct" == "$out_ws" ]]; then
    echo -e "  ${GREEN}PASS${NC}  Workpace 与单项目 info 一致"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Workspace 与单项目 info 不一致"
    echo "        direct: $out_direct"
    echo "        ws:     $out_ws"
    FAIL=$((FAIL + 1))
fi

out_direct=$("$EMB" --ai keil defines "$APPLI_PROJ" -t "Test_Appli" 2>&1 | tr -d '\r' | sort)
out_ws=$("$EMB" --ai keil defines "$WORKSPACE" -t "Test_Appli" -p "Test_Appli.uvprojx" 2>&1 | tr -d '\r' | sort)
if [[ "$out_direct" == "$out_ws" ]]; then
    echo -e "  ${GREEN}PASS${NC}  Workspace 与单项目 defines 一致"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}  Workspace 与单项目 defines 不一致"
    echo "        direct: $out_direct"
    echo "        ws:     $out_ws"
    FAIL=$((FAIL + 1))
fi
echo ""

# =========================================
# 汇总
# =========================================
TOTAL=$((PASS + FAIL))
echo "========================================"
printf "  总计: %d  通过: ${GREEN}%d${NC}  失败: ${RED}%d${NC}\n" "$TOTAL" "$PASS" "$FAIL"
echo "========================================"

if [[ $FAIL -gt 0 ]]; then
    exit 1
else
    exit 0
fi
