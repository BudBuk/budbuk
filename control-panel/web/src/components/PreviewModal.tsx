import { type DataPreview } from '../api'
import Modal from './Modal'

interface Props {
  title: string
  preview: DataPreview
  onClose: () => void
}

export default function PreviewModal({ title, preview, onClose }: Props) {
  return (
    <Modal
      title={<div className="modal-title">{title}</div>}
      subtitle={`${preview.rows.length} row${preview.rows.length === 1 ? '' : 's'} · ${preview.columns.length} columns`}
      onClose={onClose}
      width={860}
    >
      <div className="data-scroll">
        <table className="data-table">
          <thead>
            <tr>
              {preview.columns.map((c) => (
                <th key={c}>{c}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {preview.rows.map((row, ri) => (
              <tr key={ri}>
                {row.map((cell, ci) => (
                  <td key={ci}>
                    {cell === null ? <span className="null">null</span> : cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {preview.rows.length === 0 && <p className="muted">No rows.</p>}
      </div>
    </Modal>
  )
}
