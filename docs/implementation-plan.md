# Dropper Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans. Implement inline, then obtain one independent read-only review. Do not commit without the user's explicit commit request.

**Goal:** Windows·macOS 개인용 영상 압축 앱과 예상/실제 절감량을 구현한다.
**Architecture:** Rust core가 분석·정책·FFmpeg 프로세스·파일 수명주기를 소유한다. Tauri IPC는 등록된 파일 ID와 고정된 설정만 받으며 React는 상태와 기본 OS 파일 선택 UI를 제공한다.
**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vite, FFmpeg.
**Spec:** `docs/design-spec.md` (승인된 설계의 작업 폴더 복사본)

## Global Constraints

한국어, 로컬 처리, 원본 보존, 임의 셸 실행 없음, 충돌 없는 출력, 추정/실제 구분, 커밋·푸시·배포 없음. 사용자가 지정한 저장소에서 작업한다. Windows 런타임 검증 없이 양 OS 검증 완료라고 주장하지 않는다.

## Review Focus

- 취소·앱 종료·오류가 원본 또는 다른 작업 파일을 변경하지 않는지.
- 예상치가 설정/파일 변경 후 재사용되거나 실제 결과로 표기되지 않는지.
- HDR·자막·다중 오디오·회전·VFR가 조용히 손실되지 않는지.
- 특수 문자 경로와 재생 시간 미상 파일을 안전하게 처리하는지.
- 번들 실행 파일이 시스템 FFmpeg 설치에 의존하지 않는지.

## Task 1: 영상 정책과 용량 계산
- [x] `policy.rs`, `media.rs`, `command.rs`와 Rust 테스트 작성. 절감률·겹치지 않는 표본·스트림별 출력 정책·HDR·명령 인자를 검증한다.
- [x] 미구현 상태에서 실패 확인 후 구현. `cargo test --no-default-features` 통과.

## Task 2: 실제 FFmpeg 작업과 파일 안전성
- [x] `process.rs`, `engine.rs`, `jobs.rs` 구현. cancel/timeout, 중첩 작업 방지, 등록 파일만 처리, 임시 출력→검증→충돌 없는 확정.
- [x] 생성한 실제 미디어로 분석·예측·압축·취소·트랙 보존·원본 불변 통합 테스트.

## Task 3: Tauri와 단순 UI
- [x] `desktop.rs` IPC, React 작업 목록/파일 추가/강도/세부 설정/예상/진행/취소/완료 UI.
- [x] UI 상태 변경·절감량 합산 테스트 및 `npm test`, `npm run build`.
- [x] 실제 렌더링과 키보드·작은 창 확인. 브라우저 미리보기는 데스크톱 API가 없음을 명시하고 가짜 압축을 실행하지 않는다.

## Task 4: 번들·문서·최종 검증
- [x] 플랫폼별 FFmpeg 번들 준비 스크립트, 라이선스·출처 고지, 앱 아이콘, Windows/macOS 빌드 구성.
- [x] macOS 앱 빌드와 실제 번들 FFmpeg 확인. Windows는 가능한 cross-build 범위를 기록하고 런타임 미확인은 명시.
- [x] 독립 리뷰 후 중요한 발견 수정, 영향받은 검사 재실행, README 사용법·실제 검증 결과 기록.

## v0.2.0 폴더 전체 압축

- [x] 선택한 폴더·하위 폴더 탐색, 주요 영상 확장자, 결과 폴더·링크 제외와 오류 안내.
- [x] 파일·폴더 혼합 드롭 중복 제거, 폴더 정보 우선, 기존 파일 등록 정보와 호환.
- [x] 기본 source/Compressed/relative 및 사용자 지정 저장 위치/rootName/relative 유지. 원본과 겹치는 출력 루트는 이름을 분리.
- [x] 탐색·분석 진행률과 취소, 순차 일괄 압축, 전체 처리 수와 실제 절감량 합계, 50개씩 목록 표시.
- [x] 500개 제한 제거, 대량 목록 비우기는 단일 명령으로 처리하고 실제 파일은 보존.
- [x] 독립 리뷰의 드롭 순서·부모 저장 위치·직접 링크 처리 문제를 실패 테스트로 재현 후 수정.
- [x] Rust 33개·UI 상태 6개·UI 화면 6개·패키징 4개 통과, Tauri 포함 clippy 통과, 3개 플랫폼 설치 파일 재생성.
- [x] macOS 앱에서 폴더를 직접 선택해 두 하위 폴더의 같은 이름 영상을 모두 압축하고 실제 출력 구조·원본 보존 확인.

## 남은 플랫폼 실행 검증

- [ ] Windows 설치·실행·WebView2 런타임 준비: Windows 환경 필요.
- [x] Intel macOS용 FFmpeg 준비·앱·DMG 교차 빌드 및 리소스 아키텍처 확인.
- [ ] Intel macOS 런타임 검증: Intel 머신 또는 Rosetta 환경 필요.
- [ ] 인증서 서명·공증: 범위 밖, 수행하지 않음.
