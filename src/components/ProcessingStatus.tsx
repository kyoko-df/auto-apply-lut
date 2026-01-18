import React from 'react';
import { invoke } from '@tauri-apps/api/core';
import './ProcessingStatus.css';

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
      case 'pending':
        return '⏳';
      case 'processing':
        return '⚡';
      case 'completed':
        return '✅';
      case 'failed':
        return '❌';
      case 'cancelled':
        return '⏹️';
      default:
        return '❓';
    }
  };

  const getStatusText = (status: ProcessingTask['status']) => {
    switch (status) {
      case 'pending':
        return '等待中';
      case 'processing':
        return '处理中';
      case 'completed':
        return '已完成';
      case 'failed':
        return '失败';
      case 'cancelled':
        return '已取消';
      default:
        return '未知';
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
      <div className={`processing-status empty ${className}`}>
        <div className="empty-state">
          <div className="empty-icon">📋</div>
          <p>暂无处理任务</p>
        </div>
      </div>
    );
  }

  return (
    <div className={`processing-status ${className}`}>
      <div className="status-header">
        <h3>处理状态</h3>
        <div className="header-actions">
          {completedTasks.length > 0 && (
            <button 
              className="clear-button"
              onClick={onClearCompleted}
              title="清除已完成任务"
            >
              清除已完成
            </button>
          )}
        </div>
      </div>

      {activeTasks.length > 0 && (
        <div className="overall-progress">
          <div className="progress-info">
            <span>总体进度</span>
            <span>{Math.round(overallProgress)}%</span>
          </div>
          <div className="progress-bar">
            <div 
              className="progress-fill"
              style={{ width: `${overallProgress}%` }}
            />
          </div>
        </div>
      )}

      <div className="tasks-container">
        {activeTasks.length > 0 && (
          <div className="active-tasks">
            <h4>进行中的任务</h4>
            {activeTasks.map(task => (
              <div key={task.id} className="task-item active">
                <div className="task-header">
                  <div className="task-info">
                    <span className="task-icon">{getStatusIcon(task.status)}</span>
                    <span className="task-name">{task.name}</span>
                    <span className="task-status">{getStatusText(task.status)}</span>
                  </div>
                  <div className="task-actions">
                    {(task.inputPath || task.outputPath) && (
                      <>
                        <button
                          className="action-button"
                          title="打开文件"
                          onClick={async () => {
                            try {
                              const path = task.outputPath || task.inputPath;
                              if (path) {
                                await invoke('open_file', { path });
                              }
                            } catch (e) {
                              console.error('打开文件失败:', e);
                              alert('打开文件失败');
                            }
                          }}
                        >
                          打开文件
                        </button>
                        <button
                          className="action-button"
                          title="打开所在文件夹"
                          onClick={async () => {
                            try {
                              const path = task.outputPath || task.inputPath;
                              if (path) {
                                const folder = path.includes('/')
                                  ? path.substring(0, path.lastIndexOf('/'))
                                  : path.substring(0, path.lastIndexOf('\\'));
                                await invoke('open_folder', { path: folder });
                              }
                            } catch (e) {
                              console.error('打开文件夹失败:', e);
                              alert('打开文件夹失败');
                            }
                          }}
                        >
                          打开文件夹
                        </button>
                      </>
                    )}
                    {task.status === 'processing' && onCancelTask && (
                      <button 
                        className="action-button cancel"
                        onClick={() => onCancelTask(task.id)}
                        title="取消任务"
                      >
                        取消
                      </button>
                    )}
                  </div>
                </div>
                
                <div className="task-progress">
                  <div className="progress-bar">
                    <div 
                      className="progress-fill"
                      style={{ width: `${task.progress}%` }}
                    />
                  </div>
                  <span className="progress-text">{Math.round(task.progress)}%</span>
                </div>
                
                <div className="task-details">
                  <span className="task-stage">{task.stage}</span>
                  {task.eta && (
                    <span className="task-eta">剩余: {formatTime(task.eta)}</span>
                  )}
                  {task.speed && (
                    <span className="task-speed">速度: {formatSpeed(task.speed)}</span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {completedTasks.length > 0 && (
          <div className="completed-tasks">
            <h4>已完成的任务</h4>
            {completedTasks.map(task => (
              <div key={task.id} className={`task-item completed ${task.status}`}>
                <div className="task-header">
                  <div className="task-info">
                    <span className="task-icon">{getStatusIcon(task.status)}</span>
                    <span className="task-name">{task.name}</span>
                    <span className="task-status">{getStatusText(task.status)}</span>
                  </div>
                  <div className="task-actions">
                    {(task.outputPath || task.inputPath) && (
                      <>
                        <button
                          className="action-button"
                          title="打开文件"
                          onClick={async () => {
                            try {
                              const path = task.outputPath || task.inputPath;
                              if (path) {
                                await invoke('open_file', { path });
                              }
                            } catch (e) {
                              console.error('打开文件失败:', e);
                              alert('打开文件失败');
                            }
                          }}
                        >
                          打开文件
                        </button>
                        <button
                          className="action-button"
                          title="打开所在文件夹"
                          onClick={async () => {
                            try {
                              const path = task.outputPath || task.inputPath;
                              if (path) {
                                const folder = path.includes('/')
                                  ? path.substring(0, path.lastIndexOf('/'))
                                  : path.substring(0, path.lastIndexOf('\\'));
                                await invoke('open_folder', { path: folder });
                              }
                            } catch (e) {
                              console.error('打开文件夹失败:', e);
                              alert('打开文件夹失败');
                            }
                          }}
                        >
                          打开文件夹
                        </button>
                      </>
                    )}
                    {task.status === 'failed' && onRetryTask && (
                      <button 
                        className="action-button retry"
                        onClick={() => onRetryTask(task.id)}
                        title="重试任务"
                      >
                        重试
                      </button>
                    )}
                  </div>
                </div>
                
                {task.status === 'completed' && (
                  <div className="task-progress">
                    <div className="progress-bar">
                      <div className="progress-fill completed" style={{ width: '100%' }} />
                    </div>
                    <span className="progress-text">100%</span>
                  </div>
                )}
                
                {task.error && (
                  <div className="task-error">
                    <span className="error-label">错误:</span>
                    <span className="error-message">{task.error}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ProcessingStatus;