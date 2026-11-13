"""Executed inside native LLDB, after its -- executable/args are configured."""
import json
import os
from pathlib import Path
import lldb  # pyright: ignore[reportMissingImports] -- provided only inside LLDB


def capture():
    debugger = lldb.debugger
    debugger.SetAsync(False)
    debugger.HandleCommand('run')
    target = debugger.GetSelectedTarget()
    process = target.GetProcess()
    state = process.GetState()
    result = {
        'pid': process.GetProcessID(),
        'launch_observed': process.IsValid() and process.GetProcessID() > 0,
        'state': lldb.SBDebugger.StateAsCString(state),
        'exit_status': process.GetExitStatus() if state == lldb.eStateExited else None,
        'exit_description': process.GetExitDescription(),
        'threads': [],
        'debugger_control_not_product_acceptance': True,
    }
    if state in (lldb.eStateStopped, lldb.eStateCrashed, lldb.eStateSuspended):
        for thread in process:
            result['threads'].append({
                'id': thread.GetThreadID(),
                'stop_reason': thread.GetStopReason(),
                'stop_description': thread.GetStopDescription(2048),
                'frames': [str(frame) for frame in thread],
            })
        output = lldb.SBCommandReturnObject()
        debugger.GetCommandInterpreter().HandleCommand('thread backtrace all', output)
        result['all_thread_backtrace'] = output.GetOutput()
        result['backtrace_error'] = output.GetError()
        print(result['all_thread_backtrace'])
    result['signal_or_exception_captured'] = any(
        thread['stop_reason'] in (lldb.eStopReasonSignal, lldb.eStopReasonException)
        for thread in result['threads'])
    destination = Path(os.environ['ENTROPY_LLDB_RECEIPT'])
    destination.write_text(json.dumps(result, indent=2))
    print('MACDIAG_LLDB_RECEIPT ' + json.dumps(result))
    if state in (lldb.eStateStopped, lldb.eStateCrashed, lldb.eStateSuspended):
        process.Kill()


capture()
