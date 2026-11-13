import {useEffect,useRef} from 'react';
import {ArrowDownToLine,ArrowRight,ShieldCheck} from 'lucide-react';

type Props={progress:{percent:number;finished:number;total:number};cancelling:boolean;hasError:boolean;onDetails:()=>void;onCancel:()=>void};

export default function WorkView({progress,cancelling,hasError,onDetails,onCancel}:Props){
  const workViewRef=useRef<HTMLElement>(null);
  useEffect(()=>{workViewRef.current?.focus();},[]);
  return <div className="app-shell work-shell">
    <header className="app-header"><div className="brand"><span className="brand-mark" aria-hidden="true"><ArrowDownToLine size={23}/></span><span>dropper<span className="brand-period">.</span></span></div><span className="local-indicator"><ShieldCheck size={15}/>내 컴퓨터에서만</span></header>
    <main className="work-view" aria-label="작업 진행" tabIndex={-1} ref={workViewRef}>
      <span className="work-symbol" aria-hidden="true"><ArrowDownToLine size={34} strokeWidth={1.4}/></span>
      <h1>{cancelling?'안전하게 멈추는 중…':'작업 중…'}</h1>
      <div className="work-percent">{progress.percent}<span>%</span></div>
      <progress max={100} value={progress.percent} aria-label="전체 압축 진행률"/>
      <p className="work-count" role="status">{progress.total}개 중 {progress.finished}개 완료</p>
      <p className="work-hint">잠시 다른 일을 하셔도 좋아요.<br/>원본은 그대로 보관하고 있어요.</p>
      <div className="work-actions"><button className="button button-primary" onClick={onDetails}>자세히 보기<ArrowRight size={15}/></button><button className="text-button" disabled={cancelling} onClick={onCancel}>{cancelling?'취소 중…':'작업 취소'}</button></div>
      {hasError&&<p role="alert">작업 제어에 문제가 생겼습니다. 자세히 보기에서 확인해 주세요.</p>}
    </main><div className="app-bottom"><span>드롭퍼</span><span>가벼워지는 작은 습관.</span><span>v0.2.3</span></div>
  </div>;
}
