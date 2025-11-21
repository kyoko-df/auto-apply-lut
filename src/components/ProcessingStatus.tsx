import React from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Clock, Activity, CheckCircle, XCircle, AlertCircle, FolderOpen, FileVideo, X, RotateCcw } from 'lucide-react';

interface ProcessingTask {
  id: string;
  name: string;
  progress: number;
  status: 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled';
  stage: string;
  eta?: number;
  speed?: number;
  error?: string;
  inputPath?: string;
  outputPath?: string;
}

interface ProcessingStatusProps {
  tasks: ProcessingTask[];
  onCancelTask?: (taskId: string) => void;
  onRetryTask?: (taskId: string) => void;
  onClearCompleted?: () => void;
  className?: string;
}

const ProcessingStatus: React.FC<ProcessingStatusProps> = ({
  tasks,
  onCancelTask,
  onRetryTask,
  onClearCompleted,
  className = ''
}) => {
  const formatTime = (seconds: number): string => {
    if (seconds < 60) {
      return `${Math.round(seconds)}秒`;
    } else if (seconds < 3600) {
      const minutes = Math.floor(seconds / 60);
      const remainingSeconds = Math.round(seconds % 60);
      return `${minutes}分${remainingSeconds}秒`;
    } else {
      const hours = Math.floor(seconds / 3600);
      const minutes = Math.floor((seconds % 3600) / 60);
      return `${hours}小时${minutes}分钟`;
    }
  };

  const formatSpeed = (speed: number): string => {
    if (speed < 1) {
      return `${(speed * 1000).toFixed(0)}ms/frame`;
    } else {
      return `${speed.toFixed(1)}x`;
    }
  };

  const getStatusIcon = (status: ProcessingTask['status']) => {
    switch (status) {
      case 'pending': return <Clock size={16} className="text-[var(--color-text-tertiary)]" />;
      case 'processing': return <Activity size={16} className="text-[var(--color-accent)] animate-pulse" />;
      case 'completed': return <CheckCircle size={16} className="text-[var(--color-success)]" />;
      case 'failed': return <AlertCircle size={16} className="text-[var(--color-danger)]" />;
      case 'cancelled': return <XCircle size={16} className="text-[var(--color-text-tertiary)]" />;
      default: return <AlertCircle size={16} className="text-[var(--color-text-tertiary)]" />;
    }
  };

  const activeTasks = tasks.filter(task =>
    task.status === 'pending' || task.status === 'processing'
  );
  const completedTasks = tasks.filter(task =>
    task.status === 'completed' || task.status === 'failed' || task.status === 'cancelled'
  );

  const overallProgress = tasks.length > 0
    ? tasks.reduce((sum, task) => sum + task.progress, 0) / tasks.length
    : 0;

  if (tasks.length === 0) {
    return (
      <div className={`flex flex-col items-center justify-center p-8 text-[var(--color-text-tertiary)] ${className}`}>
        <div className="p-4 rounded-full bg-[var(--color-surface)] mb-3">
          <Activity size={24} />
        </div>
        <p className="text-sm">暂无处理任务</p>
      </div>
    );
  }

  return (
    <div className={`space-y-6 ${className}`}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">处理状态</h3>
        {completedTasks.length > 0 && onClearCompleted && (
          <button
            onClick={onClearCompleted}
            className="text-xs text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] transition-colors"
          >
            清除已完成
          </button>
        )}
      </div>

      {activeTasks.length > 0 && (
        <div className="apple-card bg-[var(--color-surface)]">
          <div className="flex items-center justify-between mb-3">
            <h4 className="text-sm font-medium text-[var(--color-text-secondary)]">总体进度</h4>
            <span className="text-sm font-semibold text-[var(--color-text-primary)]">{Math.round(overallProgress)}%</span>
          </div>
          <div className="h-2 bg-[var(--color-background)] rounded-full overflow-hidden">
            <div
              className="h-full bg-[var(--color-accent)] transition-all duration-300 ease-apple"
              style={{ width: `${overallProgress}%` }}
            />
          </div>
        </div>
      )}

      <div className="space-y-3">
        {activeTasks.map(task => (
          <div key={task.id} className="apple-card bg-[var(--color-surface)]">
            <div className="flex items-start justify-between mb-3">
              <div className="flex items-center gap-3">
                {getStatusIcon(task.status)}
                <div>
                  <div className="text-sm font-medium text-[var(--color-text-primary)]">{task.name}</div>
                  <div className="text-xs text-[var(--color-text-secondary)] mt-0.5 flex items-center gap-2">
                    <span>{task.stage}</span>
                    {task.eta && <span>• 剩余 {formatTime(task.eta)}</span>}
                    {task.speed && <span>• {formatSpeed(task.speed)}</span>}
                  </div>
                </div>
              </div>
              {task.status === 'processing' && onCancelTask && (
                <button
                  onClick={() => onCancelTask(task.id)}
                  className="p-1 hover:bg-[var(--color-background)] rounded text-[var(--color-text-tertiary)] hover:text-[var(--color-danger)] transition-colors"
                >
                  <X size={14} />
                </button>
              )}
            </div>

            <div className="h-1.5 bg-[var(--color-background)] rounded-full overflow-hidden">
              <div
                className="h-full bg-[var(--color-accent)] transition-all duration-300 ease-apple"
                style={{ width: `${task.progress}%` }}
              />
            </div>
          </div>
        ))}

        {completedTasks.map(task => (
          <div key={task.id} className="apple-card bg-[var(--color-surface)] opacity-80 hover:opacity-100 transition-opacity">
            <div className="flex items-start justify-between">
              <div className="flex items-center gap-3">
                {getStatusIcon(task.status)}
                <div>
                  <div className="text-sm font-medium text-[var(--color-text-primary)]">{task.name}</div>
                  <div className="text-xs text-[var(--color-text-secondary)] mt-0.5">
                    {task.error ? (
                      <span className="text-[var(--color-danger)]">{task.error}</span>
                    ) : (
                      '已完成'
                    )}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-1">
                {(task.outputPath || task.inputPath) && (
                  <>
                    <button
                      onClick={() => {
                        const path = task.outputPath || task.inputPath;
                        if (path) invoke('open_file', { path });
                      }}
                      className="p-1.5 hover:bg-[var(--color-background)] rounded text-[var(--color-text-secondary)] transition-colors"
                      title="打开文件"
                    >
                      <FileVideo size={14} />
                    </button>
                    <button
                      onClick={() => {
                        const path = task.outputPath || task.inputPath;
                        if (path) {
                          const folder = path.includes('/')
                            ? path.substring(0, path.lastIndexOf('/'))
                            : path.substring(0, path.lastIndexOf('\\'));
                          invoke('open_folder', { path: folder });
                        }
                      }}
                      className="p-1.5 hover:bg-[var(--color-background)] rounded text-[var(--color-text-secondary)] transition-colors"
                      title="所在文件夹"
                    >
                      <FolderOpen size={14} />
                    </button>
                  </>
                )}
                {task.status === 'failed' && onRetryTask && (
                  <button
                    onClick={() => onRetryTask(task.id)}
                    className="p-1.5 hover:bg-[var(--color-background)] rounded text-[var(--color-text-secondary)] hover:text-[var(--color-primary)] transition-colors"
                    title="重试"
                  >
                    <RotateCcw size={14} />
                  </button>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default ProcessingStatus;