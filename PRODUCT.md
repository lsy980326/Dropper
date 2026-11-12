# Dropper

<!-- impeccable:product-schema 1 -->

## Platform
web

Tauri로 패키징하는 Windows·macOS 데스크톱 도구. 웹 서비스가 아니며 로컬에서 처리한다.

## Stack
사용자가 승인한 설계: Tauri 2, Rust, React, TypeScript, Vite, FFmpeg/ffprobe.

## Users
개인 사용자가 자주 쓰는 영상의 용량을 적은 조작으로 줄인다.

## Product Purpose
원본을 보존하면서 영상의 무게를 덜어내고 예상·실제 절감량을 분명하게 보여준다.

## Capabilities and Constraints
여러 파일·폴더 추가, 하위 폴더의 영상 재귀 탐색, 폴더 구조 유지, 세 단계 품질, 시험 압축 기반 예측, 순차 압축, 취소, 실제 절감량. 폴더 안의 영상 수에 500개 제한을 두지 않으며 큰 목록은 50개씩 표시하되 전체를 처리한다. 한국어 UI. 서버 업로드·계정·원본 삭제 없음. 모든 확장자 내부의 모든 코덱이나 압축률을 보장하지 않음.

## Brand Commitments
이름은 드롭퍼 / Dropper. 문구: 영상의 무게를 덜어내다. 간편하고 복잡하지 않은 UI.

## Product Principles
- 기본 흐름은 파일 추가 → 강도 선택 → 압축.
- 원본 안전성과 실제 결과가 먼저다.
- 예측과 확정 결과를 구분한다.
- 전문가 설정은 접어서 제공한다.
